//! Registering a lot: the first use case in this project that writes.
//!
//! Everything arriving here is raw text typed by a person, so this module
//! is the trust boundary. Nothing reaches [`CuratedDataWriter`] before it
//! has been parsed into a domain type and range-checked, and a lot is
//! never written twice under the same id.

use std::str::FromStr;

use crate::core::domain::{Depth, DomainError, FieldContext, IrrigationSystem, Nutrient, SoilTest, Texture, YieldTarget};
use crate::core::ports::{CuratedDataWriter, FieldContextRepository, RegisterLotPort};

/// A lot as typed by a user: every field is raw text, including the
/// numbers, so that parsing and validation happen in exactly one place
/// instead of once per front-end.
///
/// `latitude`/`longitude` are optional (empty means "not surveyed"); so
/// are `crop_id`/`yield_value`, which write the lot's first planning row
/// when both are given.
#[derive(Debug, Clone, Default)]
pub struct LotRegistration {
    pub field_id: String,
    pub texture: String,
    pub irrigation_system: String,
    pub organic_matter_percent: String,
    pub ph: String,
    pub cec_cmolc_kg: String,
    pub bulk_density_kg_dm3: String,
    pub arable_depth_m: String,
    pub region: String,
    pub latitude: String,
    pub longitude: String,
    pub crop_id: String,
    pub yield_value: String,
    pub yield_unit: String,
}

/// One lab result as typed by a user, for an existing lot.
#[derive(Debug, Clone, Default)]
pub struct SoilTestEntry {
    pub nutrient_id: String,
    pub value: String,
    pub unit: String,
    pub method: String,
    pub depth_from_cm: String,
    pub depth_to_cm: String,
}

pub struct RegisterLot {
    field_context: Box<dyn FieldContextRepository>,
    writer: Box<dyn CuratedDataWriter>,
}

impl RegisterLot {
    pub fn new(field_context: Box<dyn FieldContextRepository>, writer: Box<dyn CuratedDataWriter>) -> Self {
        Self { field_context, writer }
    }
}

impl RegisterLotPort for RegisterLot {
    fn register_lot(&self, registration: &LotRegistration) -> Result<(), DomainError> {
        let field_id = required("field_id", &registration.field_id)?;
        // A duplicate id would shadow the existing lot on every read (all
        // curated readers stop at the first match), so it is refused before
        // anything is written rather than "fixed" by overwriting.
        if self.field_context.get_context_by_field_id(&field_id).is_ok() {
            return Err(DomainError::InvalidInput(format!("lot {field_id} already exists")));
        }

        let context = FieldContext {
            // One lot, one composite sample, one context row — the same
            // identity the CLI's `--lot` already assumes.
            sample_id: field_id.clone(),
            texture: Texture::from_str(&registration.texture)?,
            irrigation_system: IrrigationSystem::from_str(&registration.irrigation_system)?,
            organic_matter_percent: percentage("organic_matter_percent", &registration.organic_matter_percent)?,
            ph: bounded("ph", &registration.ph, 0.0, 14.0)?,
            cec_cmolc_kg: positive("cec", &registration.cec_cmolc_kg)?,
            bulk_density_kg_dm3: positive("bulk_density_kg_dm3", &registration.bulk_density_kg_dm3)?,
            arable_depth_m: positive("arable_depth_m", &registration.arable_depth_m)?,
            region: required("region", &registration.region)?,
            latitude: optional_bounded("latitude", &registration.latitude, -90.0, 90.0)?,
            longitude: optional_bounded("longitude", &registration.longitude, -180.0, 180.0)?,
            field_id,
        };

        // The planning row is optional, but if half of it was typed the
        // user meant to type the other half — silently dropping it would
        // hide the mistake until the first plan fails.
        let target = match (registration.crop_id.trim(), registration.yield_value.trim()) {
            ("", "") => None,
            (crop_id, value) => Some((
                required("crop_id", crop_id)?,
                YieldTarget {
                    value: positive("yield_value", value)?,
                    unit: required("yield_unit", &registration.yield_unit)?,
                },
            )),
        };

        self.writer.save_field_context(&context)?;
        if let Some((crop_id, target)) = target {
            self.writer.save_yield_target(&context.field_id, &crop_id, &target)?;
        }
        Ok(())
    }

    fn add_soil_tests(&self, field_id: &str, entries: &[SoilTestEntry]) -> Result<(), DomainError> {
        let field_id = required("field_id", field_id)?;
        // The mirror image of `register_lot`'s check: a sample is only
        // meaningful attached to a lot that exists.
        let context = self.field_context.get_context_by_field_id(&field_id)?;
        if entries.is_empty() {
            return Err(DomainError::InvalidInput("no soil test to save".to_string()));
        }

        let tests = entries
            .iter()
            .map(|entry| {
                let from_cm = bounded("depth_from_cm", &entry.depth_from_cm, 0.0, f64::MAX)?;
                let to_cm = bounded("depth_to_cm", &entry.depth_to_cm, 0.0, f64::MAX)?;
                if to_cm <= from_cm {
                    return Err(DomainError::InvalidInput(format!(
                        "depth_to_cm ({to_cm}) must be deeper than depth_from_cm ({from_cm})"
                    )));
                }
                Ok(SoilTest {
                    sample_id: context.sample_id.clone(),
                    nutrient: Nutrient::from_str(&entry.nutrient_id)?,
                    // A lab value of zero is a real reading ("below
                    // detection"), unlike a bulk density of zero.
                    value: bounded("value", &entry.value, 0.0, f64::MAX)?,
                    unit: required("unit", &entry.unit)?,
                    method: required("method", &entry.method)?,
                    layer: Depth { from_cm, to_cm },
                })
            })
            .collect::<Result<Vec<_>, DomainError>>()?;

        self.writer.save_soil_tests(&tests)
    }
}

// ---- validation helpers ---------------------------------------------------

fn required(field: &str, value: &str) -> Result<String, DomainError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(DomainError::InvalidInput(format!("{field} is required")));
    }
    Ok(value.to_string())
}

fn number(field: &str, value: &str) -> Result<f64, DomainError> {
    let parsed: f64 = value
        .trim()
        .parse()
        .map_err(|_| DomainError::InvalidInput(format!("{field} must be a number, got {:?}", value.trim())))?;
    if !parsed.is_finite() {
        return Err(DomainError::InvalidInput(format!("{field} must be a finite number")));
    }
    Ok(parsed)
}

fn positive(field: &str, value: &str) -> Result<f64, DomainError> {
    let parsed = number(field, value)?;
    if parsed <= 0.0 {
        return Err(DomainError::InvalidInput(format!("{field} must be greater than zero, got {parsed}")));
    }
    Ok(parsed)
}

fn bounded(field: &str, value: &str, min: f64, max: f64) -> Result<f64, DomainError> {
    let parsed = number(field, value)?;
    if parsed < min || parsed > max {
        return Err(DomainError::InvalidInput(format!("{field} must be between {min} and {max}, got {parsed}")));
    }
    Ok(parsed)
}

fn percentage(field: &str, value: &str) -> Result<f64, DomainError> {
    bounded(field, value, 0.0, 100.0)
}

fn optional_bounded(field: &str, value: &str, min: f64, max: f64) -> Result<Option<f64>, DomainError> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    bounded(field, value, min, max).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Knows about exactly one lot, and records nothing — the tests here
    /// are about what gets rejected, and what shape reaches the writer.
    struct OneLotRepo;

    impl FieldContextRepository for OneLotRepo {
        fn get_context_by_field_id(&self, field_id: &str) -> Result<FieldContext, DomainError> {
            if field_id != "LOT-EXISTING" {
                return Err(DomainError::NotFound(format!("no field context for field_id={field_id}")));
            }
            Ok(FieldContext {
                field_id: field_id.to_string(),
                sample_id: field_id.to_string(),
                texture: Texture::Loam,
                irrigation_system: IrrigationSystem::Rainfed,
                organic_matter_percent: 3.2,
                ph: 6.3,
                cec_cmolc_kg: 12.0,
                bulk_density_kg_dm3: 1.3,
                arable_depth_m: 0.2,
                region: "global".to_string(),
                latitude: None,
                longitude: None,
            })
        }

        fn list_contexts(&self) -> Result<Vec<FieldContext>, DomainError> {
            Ok(vec![self.get_context_by_field_id("LOT-EXISTING")?])
        }
    }

    #[derive(Default)]
    struct SpyWriter {
        contexts: RefCell<Vec<FieldContext>>,
        tests: RefCell<Vec<SoilTest>>,
        targets: RefCell<Vec<(String, String, YieldTarget)>>,
    }

    impl CuratedDataWriter for SpyWriter {
        fn save_field_context(&self, context: &FieldContext) -> Result<(), DomainError> {
            self.contexts.borrow_mut().push(context.clone());
            Ok(())
        }

        fn save_soil_tests(&self, tests: &[SoilTest]) -> Result<(), DomainError> {
            self.tests.borrow_mut().extend_from_slice(tests);
            Ok(())
        }

        fn save_yield_target(&self, field_id: &str, crop_id: &str, target: &YieldTarget) -> Result<(), DomainError> {
            self.targets
                .borrow_mut()
                .push((field_id.to_string(), crop_id.to_string(), target.clone()));
            Ok(())
        }
    }

    fn valid() -> LotRegistration {
        LotRegistration {
            field_id: "LOT-003".to_string(),
            texture: "sandy_loam".to_string(),
            irrigation_system: "sprinkler".to_string(),
            organic_matter_percent: "4.1".to_string(),
            ph: "5.9".to_string(),
            cec_cmolc_kg: "14".to_string(),
            bulk_density_kg_dm3: "1.25".to_string(),
            arable_depth_m: "0.25".to_string(),
            region: "global".to_string(),
            ..Default::default()
        }
    }

    /// So a test can keep looking at the spy after handing it to the use
    /// case, which takes ownership of its writer.
    impl CuratedDataWriter for Rc<SpyWriter> {
        fn save_field_context(&self, context: &FieldContext) -> Result<(), DomainError> {
            (**self).save_field_context(context)
        }

        fn save_soil_tests(&self, tests: &[SoilTest]) -> Result<(), DomainError> {
            (**self).save_soil_tests(tests)
        }

        fn save_yield_target(&self, field_id: &str, crop_id: &str, target: &YieldTarget) -> Result<(), DomainError> {
            (**self).save_yield_target(field_id, crop_id, target)
        }
    }

    fn use_case(spy: &Rc<SpyWriter>) -> RegisterLot {
        RegisterLot::new(Box::new(OneLotRepo), Box::new(Rc::clone(spy)))
    }

    fn register(registration: &LotRegistration) -> Result<(), DomainError> {
        use_case(&Rc::new(SpyWriter::default())).register_lot(registration)
    }

    #[test]
    fn a_valid_lot_is_parsed_into_domain_types_before_being_written() {
        let spy = Rc::new(SpyWriter::default());

        use_case(&spy).register_lot(&valid()).expect("a valid lot registers");

        let contexts = spy.contexts.borrow();
        assert_eq!(contexts.len(), 1);
        assert_eq!(contexts[0].texture, Texture::SandyLoam);
        assert_eq!(contexts[0].irrigation_system, IrrigationSystem::Sprinkler);
        assert_eq!(contexts[0].sample_id, "LOT-003", "a lot is its own composite sample");
        assert_eq!(contexts[0].latitude, None, "an empty coordinate is absent, not zero");
        assert!(spy.targets.borrow().is_empty(), "no crop was given, so no planning row");
    }

    #[test]
    fn a_crop_and_a_goal_together_also_write_the_planning_row() {
        let spy = Rc::new(SpyWriter::default());
        let mut registration = valid();
        registration.crop_id = "corn".to_string();
        registration.yield_value = "9.5".to_string();
        registration.yield_unit = "t_ha".to_string();

        use_case(&spy).register_lot(&registration).expect("a valid lot registers");

        let targets = spy.targets.borrow();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].0, "LOT-003");
        assert_eq!(targets[0].1, "corn");
        assert_eq!(targets[0].2.value, 9.5);
    }

    #[test]
    fn a_duplicate_field_id_is_refused_before_anything_is_written() {
        let spy = Rc::new(SpyWriter::default());
        let mut registration = valid();
        registration.field_id = "LOT-EXISTING".to_string();

        let outcome = use_case(&spy).register_lot(&registration);

        assert!(matches!(outcome, Err(DomainError::InvalidInput(_))), "{outcome:?}");
        assert!(spy.contexts.borrow().is_empty(), "a refused lot must not reach the writer");
    }

    #[test]
    fn unparseable_and_out_of_range_values_are_refused_one_by_one() {
        let cases: Vec<(&str, fn(&mut LotRegistration))> = vec![
            ("blank id", |r| r.field_id = "  ".to_string()),
            ("unknown texture", |r| r.texture = "chocolate".to_string()),
            ("unknown irrigation", |r| r.irrigation_system = "hose".to_string()),
            ("negative organic matter", |r| r.organic_matter_percent = "-1".to_string()),
            ("organic matter over 100%", |r| r.organic_matter_percent = "120".to_string()),
            ("pH off the scale", |r| r.ph = "63".to_string()),
            ("zero bulk density", |r| r.bulk_density_kg_dm3 = "0".to_string()),
            ("non-numeric depth", |r| r.arable_depth_m = "deep".to_string()),
            ("blank region", |r| r.region = String::new()),
            ("impossible latitude", |r| r.latitude = "120".to_string()),
            ("a crop with no yield goal", |r| r.crop_id = "corn".to_string()),
            ("a yield goal with no crop", |r| r.yield_value = "9.5".to_string()),
        ];

        for (name, break_it) in cases {
            let mut registration = valid();
            break_it(&mut registration);
            let outcome = register(&registration);
            assert!(
                matches!(outcome, Err(DomainError::InvalidInput(_))),
                "{name} should have been refused, got {outcome:?}"
            );
        }
    }

    #[test]
    fn a_sample_can_only_be_added_to_a_lot_that_exists() {
        let use_case = RegisterLot::new(Box::new(OneLotRepo), Box::new(SpyWriter::default()));
        let entry = SoilTestEntry {
            nutrient_id: "P".to_string(),
            value: "18".to_string(),
            unit: "mg_per_kg".to_string(),
            method: "Olsen".to_string(),
            depth_from_cm: "0".to_string(),
            depth_to_cm: "20".to_string(),
        };

        assert!(use_case.add_soil_tests("LOT-EXISTING", &[entry.clone()]).is_ok());
        assert!(matches!(
            use_case.add_soil_tests("LOT-NOWHERE", &[entry.clone()]),
            Err(DomainError::NotFound(_))
        ));
        assert!(matches!(use_case.add_soil_tests("LOT-EXISTING", &[]), Err(DomainError::InvalidInput(_))));

        let mut inverted = entry.clone();
        inverted.depth_to_cm = "0".to_string();
        assert!(matches!(
            use_case.add_soil_tests("LOT-EXISTING", &[inverted]),
            Err(DomainError::InvalidInput(_))
        ));

        let mut unknown_nutrient = entry;
        unknown_nutrient.nutrient_id = "Kryptonite".to_string();
        assert!(matches!(
            use_case.add_soil_tests("LOT-EXISTING", &[unknown_nutrient]),
            Err(DomainError::InvalidInput(_))
        ));
    }
}
