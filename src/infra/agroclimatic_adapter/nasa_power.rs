//! NASA POWER client: 30-year monthly climatology for a point.
//!
//! No API key. The response is a monthly series per parameter, keyed
//! `"JAN"`..`"DEC"` plus an `"ANN"` aggregate; this adapter reduces it to
//! the annual figures the domain consumes and throws the rest away.
//!
//! API docs: <https://power.larc.nasa.gov/docs/services/api/temporal/climatology/>
//!
//! The wire-format structs and the sentinel handling below follow the
//! same shape as the sibling `vigil` project's POWER client.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::Deserialize;

use crate::core::domain::{AnnualClimatology, DomainError};
use crate::core::ports::AgroclimaticRepository;

const BASE_URL: &str = "https://power.larc.nasa.gov/api/temporal/climatology/point";

/// POWER's fill value for a cell it has no data for. It arrives as a real
/// number in the JSON, so it has to be filtered rather than deserialized
/// away. Compared with a margin instead of `== -999.0`, because a float
/// that survived a JSON round-trip is not guaranteed to compare equal.
const FILL: f64 = -999.0;

/// Requested parameters, in POWER's vocabulary.
///
/// `ET0_PENMAN` from the original brief is **not** requested: no such
/// parameter exists in the AG community, and including it makes POWER
/// reject the entire request with HTTP 422 rather than return a fill
/// value for that one field. `TOA_SW_DWN` is requested in its place, as
/// the Ra term of a Hargreaves ET0 (see `climatology_from`).
const PARAMETERS: &str = "PRECTOTCORR,T2M,T2M_MAX,T2M_MIN,ALLSKY_SFC_SW_DWN,RH2M,WS2M,TOA_SW_DWN";

const TIMEOUT: Duration = Duration::from_secs(10);

pub struct NasaPowerRepo {
    http: reqwest::blocking::Client,
    base_url: String,
}

impl NasaPowerRepo {
    pub fn new() -> Result<Self, DomainError> {
        let http = reqwest::blocking::Client::builder()
            .timeout(TIMEOUT)
            .build()
            .map_err(|e| DomainError::ExternalServiceUnavailable(format!("could not build http client: {e}")))?;
        Ok(Self { http, base_url: BASE_URL.to_string() })
    }

    /// Point the client at a different host — used by the tests to serve
    /// a canned response without reaching the real API.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }
}

impl AgroclimaticRepository for NasaPowerRepo {
    fn fetch_climatology(&self, latitude: f64, longitude: f64) -> Result<AnnualClimatology, DomainError> {
        let response = self
            .http
            .get(&self.base_url)
            .query(&[
                ("latitude", format!("{latitude:.4}")),
                ("longitude", format!("{longitude:.4}")),
                ("community", "AG".to_string()),
                ("parameters", PARAMETERS.to_string()),
                ("format", "JSON".to_string()),
            ])
            .send()
            .map_err(|e| DomainError::ExternalServiceUnavailable(format!("NASA POWER request failed: {e}")))?;

        // POWER answers a bad request with 422 and a JSON *error* body, so
        // the status has to be checked before decoding — otherwise the
        // failure surfaces as a confusing missing-field parse error.
        let status = response.status();
        if !status.is_success() {
            return Err(DomainError::ExternalServiceUnavailable(format!("NASA POWER returned HTTP {}", status.as_u16())));
        }

        let body = response
            .text()
            .map_err(|e| DomainError::ExternalServiceUnavailable(format!("could not read NASA POWER response: {e}")))?;

        climatology_from(&body)
    }
}

/// Parses a POWER climatology payload into the domain struct. Split out
/// from the HTTP call so the reduction logic is testable offline.
pub fn climatology_from(body: &str) -> Result<AnnualClimatology, DomainError> {
    let raw: PowerResponse = serde_json::from_str(body)
        .map_err(|e| DomainError::ExternalServiceUnavailable(format!("could not decode NASA POWER response: {e}")))?;
    let parameters = raw.properties.parameter;

    // POWER's own "ANN" aggregate, when the parameter has one.
    let annual = |key: &str| parameters.get(key).and_then(|months| months.get("ANN")).copied().filter(|v| *v > FILL);
    // Reduce over the twelve months, ignoring the "ANN" entry. Used where
    // the annual extreme is the agronomically meaningful figure and where
    // POWER's own aggregate can't be assumed to be a max/min.
    let over_months = |key: &str, pick: fn(f64, f64) -> f64| {
        parameters.get(key).and_then(|months| {
            months
                .iter()
                .filter(|(month, _)| month.as_str() != "ANN")
                .map(|(_, v)| *v)
                .filter(|v| *v > FILL)
                .reduce(pick)
        })
    };

    // Hargreaves ET0, computed per month and then averaged. Per month
    // because the equation's (Tmax - Tmin) term must be a within-period
    // diurnal range: feeding it the annual spread between the hottest and
    // coldest months would badly overstate evaporative demand.
    let et0_mm_per_day = monthly_et0_mean(&parameters);

    Ok(AnnualClimatology {
        mean_temp_c: annual("T2M"),
        max_temp_c: over_months("T2M_MAX", f64::max),
        min_temp_c: over_months("T2M_MIN", f64::min),
        precip_mm_per_day: annual("PRECTOTCORR"),
        solar_mj_m2_per_day: annual("ALLSKY_SFC_SW_DWN"),
        humidity_pct: annual("RH2M"),
        wind_ms: annual("WS2M"),
        et0_mm_per_day,
    })
}

/// Mean of the twelve monthly Hargreaves ET0 values. `None` unless every
/// input parameter is present — a partial year would silently bias the
/// annual total that the water-deficit rule compares against rainfall.
fn monthly_et0_mean(parameters: &BTreeMap<String, BTreeMap<String, f64>>) -> Option<f64> {
    let mean = parameters.get("T2M")?;
    let max = parameters.get("T2M_MAX")?;
    let min = parameters.get("T2M_MIN")?;
    let ra = parameters.get("TOA_SW_DWN")?;

    let mut total = 0.0;
    let mut months = 0u32;
    for (month, mean_temp) in mean.iter().filter(|(m, _)| m.as_str() != "ANN") {
        let (Some(max_temp), Some(min_temp), Some(ra_month)) = (max.get(month), min.get(month), ra.get(month)) else {
            continue;
        };
        if [*mean_temp, *max_temp, *min_temp, *ra_month].iter().any(|v| *v <= FILL) {
            continue;
        }
        total += crate::core::domain::services::reference_et0_hargreaves_mm_day(*mean_temp, *max_temp, *min_temp, *ra_month);
        months += 1;
    }

    (months == 12).then(|| total / months as f64)
}

// ---- Raw wire format -------------------------------------------------------

#[derive(Deserialize)]
struct PowerResponse {
    properties: PowerProperties,
}

#[derive(Deserialize)]
struct PowerProperties {
    /// Parameter code -> month key ("JAN".."DEC", "ANN") -> value.
    /// Kept as a map rather than named fields so that changing
    /// `PARAMETERS` needs no change here.
    parameter: BTreeMap<String, BTreeMap<String, f64>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed real response for LOT-001's coordinates (1.2136, -77.2811).
    /// Monthly values are the genuine ones; `MAR` carries a -999 fill in
    /// `RH2M` to exercise the sentinel path.
    const PASTO: &str = r#"{
      "properties": { "parameter": {
        "T2M": {"JAN":13.29,"FEB":13.32,"MAR":13.4,"APR":13.4,"MAY":13.37,"JUN":13.11,
                "JUL":12.79,"AUG":12.86,"SEP":13.16,"OCT":13.13,"NOV":13.14,"DEC":13.2,"ANN":13.17},
        "T2M_MAX": {"JAN":20.9,"FEB":20.8,"MAR":20.7,"APR":20.6,"MAY":20.6,"JUN":20.5,
                    "JUL":20.6,"AUG":21.0,"SEP":21.2,"OCT":20.8,"NOV":20.6,"DEC":20.8,"ANN":22.59},
        "T2M_MIN": {"JAN":6.0,"FEB":6.1,"MAR":6.2,"APR":6.3,"MAY":6.3,"JUN":5.9,
                    "JUL":5.4,"AUG":5.3,"SEP":5.6,"OCT":6.0,"NOV":6.1,"DEC":6.0,"ANN":4.42},
        "TOA_SW_DWN": {"JAN":35.0,"FEB":36.3,"MAR":37.4,"APR":37.5,"MAY":36.5,"JUN":35.7,
                       "JUL":36.0,"AUG":37.0,"SEP":37.4,"OCT":36.5,"NOV":35.1,"DEC":34.6,"ANN":35.9},
        "PRECTOTCORR": {"JAN":2.5,"FEB":2.7,"MAR":3.2,"APR":3.7,"MAY":3.1,"JUN":2.2,
                        "JUL":1.19,"AUG":1.3,"SEP":2.3,"OCT":4.3,"NOV":4.0,"DEC":3.3,"ANN":2.84},
        "ALLSKY_SFC_SW_DWN": {"JAN":14.5,"ANN":14.5},
        "RH2M": {"JAN":84.0,"MAR":-999.0,"ANN":84.78},
        "WS2M": {"JAN":1.8,"ANN":1.81}
      } }
    }"#;

    #[test]
    fn reduces_a_real_response_to_annual_figures() {
        let climate = climatology_from(PASTO).expect("parses");
        assert_eq!(climate.mean_temp_c, Some(13.17));
        assert_eq!(climate.precip_mm_per_day, Some(2.84));
        assert_eq!(climate.solar_mj_m2_per_day, Some(14.5));
        assert_eq!(climate.wind_ms, Some(1.81));
        // 2.84 mm/day over a year.
        assert!((climate.annual_precip_mm().expect("precip") - 1036.6).abs() < 0.1);
    }

    #[test]
    fn temperature_extremes_come_from_the_months_not_the_ann_aggregate() {
        let climate = climatology_from(PASTO).expect("parses");
        // POWER's own ANN entries here are 22.59 / 4.42, but the heat and
        // cold rules must see the hottest and coldest *months*: 21.2 / 5.3.
        assert_eq!(climate.max_temp_c, Some(21.2));
        assert_eq!(climate.min_temp_c, Some(5.3));
    }

    #[test]
    fn et0_is_derived_per_month_and_averaged() {
        let climate = climatology_from(PASTO).expect("parses");
        let et0 = climate.et0_mm_per_day.expect("et0 derived");
        // ~4.0 mm/day for this cell. The point of the per-month average is
        // that it stays well below what the annual 22.59/4.42 spread would
        // produce (~4.9), which would overstate evaporative demand.
        assert!((et0 - 4.02).abs() < 0.1, "got {et0}");
    }

    #[test]
    fn fill_values_become_none_rather_than_minus_999() {
        // RH2M's ANN is real, but a -999 anywhere must never leak through
        // as a number. Rebuild the payload with a filled ANN to check.
        let filled = PASTO.replace(r#""RH2M": {"JAN":84.0,"MAR":-999.0,"ANN":84.78}"#, r#""RH2M": {"JAN":84.0,"ANN":-999.0}"#);
        let climate = climatology_from(&filled).expect("parses");
        assert_eq!(climate.humidity_pct, None);
        // Unrelated parameters are unaffected.
        assert_eq!(climate.mean_temp_c, Some(13.17));
    }

    #[test]
    fn a_missing_parameter_is_none_not_an_error() {
        // A grid cell that returns no wind at all still yields a usable
        // climatology for every other rule.
        let without_wind = PASTO.replace(r#""WS2M""#, r#""SOME_OTHER_PARAMETER""#);
        let climate = climatology_from(&without_wind).expect("parses");
        assert_eq!(climate.wind_ms, None);
        assert_eq!(climate.mean_temp_c, Some(13.17));
    }

    #[test]
    fn a_partial_year_yields_no_et0() {
        // Drop one month of Ra: the annual ET0 total would be biased low,
        // so the whole figure is withheld rather than under-reported.
        let short_year = PASTO.replace(r#""DEC":34.6,"ANN":35.9"#, r#""ANN":35.9"#);
        let climate = climatology_from(&short_year).expect("parses");
        assert_eq!(climate.et0_mm_per_day, None);
        assert!(climate.mean_temp_c.is_some(), "other parameters survive");
    }

    #[test]
    fn an_error_body_is_a_decode_failure_not_a_silent_empty_climatology() {
        // What POWER actually returns for a bad parameter (HTTP 422).
        let error_body = r#"{"header":"failed","messages":["One of your parameters is incorrect: ET0_PENMAN."]}"#;
        let result = climatology_from(error_body);
        assert!(matches!(result, Err(DomainError::ExternalServiceUnavailable(_))));
    }
}
