//! Drawing that is not a table: a reading placed on the scale it was
//! judged against, and the arithmetic that produced an efficiency.
//!
//! One rule holds over everything here, and it is the only reason any of
//! it earns its space: **nothing is drawn over a figure the screen already
//! writes beside it.** A glyph that restates the number next to it is
//! decoration, and decoration in a terminal costs columns the data needed.
//! Both primitives below show something no column on the page carries —
//! *where* inside its band a value sits, and *why* an efficiency is what
//! it is.
//!
//! Nothing here holds state or reads the app. Numbers and a [`Theme`] in,
//! [`Line`]s out, which is what makes both of them testable against the
//! domain figures they claim to draw.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::theme::Theme;
use crate::core::domain::{AdjustedEfficiency, QualitativeBand};

/// The glyph the track is drawn with.
const TRACK: char = '─';

/// Where the value falls. A filled mark rather than a line so it survives
/// against a track of the same colour.
const MARK: char = '●';

/// Printed where a band runs off the end of the scale, in place of a bound
/// the table does not state.
const OPEN_LOW: char = '‹';

/// The other end of [`OPEN_LOW`].
const OPEN_HIGH: char = '›';

/// One reading placed inside the bands it was classified against.
///
/// The verdict — `low`, `slightly acid` — is a word, and a word cannot say
/// whether a value sits at the edge of the next band or in the middle of
/// its own. A K of 0.39 and a K of 0.05 both read *low* and are different
/// soils to manage; this is the difference, drawn.
///
/// Each band gets a share of the track proportional to its width, painted
/// in the role colour its position implies — the bottom band is a failure,
/// the top one is fine, everything between is something to watch. That
/// mapping is positional on purpose: the bands come from a data file and
/// their names are the source table's, so matching on them would break the
/// day a profile words a band differently.
///
/// # Arguments
/// * `theme` — the palette to take the role colours from.
/// * `value` — the reading, in the same unit the bands state their bounds
///   in. The caller is responsible for that: this function cannot convert.
/// * `bands` — every band of the table, in order. An empty slice draws
///   nothing at all rather than an empty track.
/// * `width` — cells the track may take, chrome included.
///
/// # Returns
/// One line: the track with the mark on it, bracketed by whichever ends
/// are open. Empty when there is nothing to place the reading against.
///
/// # Example
/// ```ignore
/// // K 0.25 against low/medium/high thresholds
/// let line = gauge(&theme, 0.25, &bands, 24);
/// ```
pub fn gauge<'a>(theme: &Theme, value: f64, bands: &[QualitativeBand], width: usize) -> Line<'a> {
    // ALGORITHM: the track is divided by *band*, equally, not by value.
    //
    // A linear scale cannot draw these tables. The outermost band of every
    // one of them is open-ended — `pH < 5.5` has no bottom — so it has no
    // width to be proportional to, and a linear track gives it zero cells
    // and paints readings inside it over its neighbour. Worse, a real
    // table's bands differ in span by an order of magnitude, so the wide
    // ones would squash the narrow ones flat.
    //
    // Equal shares make each band readable and make the *position inside
    // the band* — the thing this line exists to show — the same size
    // everywhere. The mark is then placed inside the band
    // `QualitativeBand::contains` picks, so the drawing uses the domain's
    // own predicate and cannot disagree with the verdict printed next to
    // it.
    if bands.is_empty() || width < 4 {
        return Line::default();
    }
    let track = width.saturating_sub(2);
    let share = track / bands.len();
    if share == 0 {
        return Line::default();
    }
    // The remainder is spread over the leading bands rather than left off
    // the end, so every gauge comes out exactly `width` cells and a column
    // of them lines up. A table with nine bands and one with three
    // otherwise end in different places, and whatever is printed after
    // them goes ragged.
    let extra = track % bands.len();
    let starts: Vec<usize> = (0..bands.len()).map(|index| index * share + index.min(extra)).collect();
    let cells_of = |index: usize| share + usize::from(index < extra);

    // A reading no band contains is genuinely unclassified — the same
    // condition that leaves `category` as `None`. It gets a track and no
    // mark rather than a mark somewhere invented.
    let here = bands.iter().position(|band| band.contains(value));
    let mark = here.map(|index| {
        let start = starts[index];
        let last = cells_of(index) as f64 - 1.0;
        let Some((low, high)) = extent(bands, index) else { return start };
        start + ((value - low) / (high - low) * last).round().clamp(0.0, last) as usize
    });

    let mut spans = vec![Span::styled(OPEN_LOW.to_string(), Style::default().fg(theme.border))];
    for (index, start) in starts.iter().enumerate() {
        let style = band_style(theme, index, bands.len());
        for cell in *start..start + cells_of(index) {
            spans.push(if Some(cell) == mark {
                Span::styled(MARK.to_string(), theme.strong())
            } else {
                Span::styled(TRACK.to_string(), style)
            });
        }
    }
    spans.push(Span::styled(OPEN_HIGH.to_string(), Style::default().fg(theme.border)));
    Line::from(spans)
}

/// How far a band reaches, for the purpose of placing a mark inside it.
///
/// A band that states both bounds reaches exactly that far. The outermost
/// band of every real table states only one — `K below 0.4` has no floor —
/// and left at that, every reading inside it would sit on the same cell:
/// a K of 0.39 and a K of 0.05 would draw identically, which is the exact
/// blindness this whole line exists to remove.
///
/// So an open band borrows the width of the band beside it. **That is a
/// drawing convention, not a claim**: the literature says nothing about
/// where *low* ends, and nothing here pretends it does. What the borrowed
/// width buys is the only distinction that matters at a glance — just past
/// the cut, or a long way past it.
///
/// # Returns
/// `None` when neither the band nor its neighbour states a usable span, in
/// which case the caller marks the band's own start.
fn extent(bands: &[QualitativeBand], index: usize) -> Option<(f64, f64)> {
    let width = |at: usize| -> Option<f64> {
        let band = bands.get(at)?;
        Some(band.max_value? - band.min_value?).filter(|span| *span > 0.0)
    };
    match (bands[index].min_value, bands[index].max_value) {
        (Some(min), Some(max)) if max > min => Some((min, max)),
        (None, Some(max)) => width(index + 1).map(|span| (max - span, max)),
        (Some(min), None) => width(index.checked_sub(1)?).map(|span| (min, min + span)),
        _ => None,
    }
}

/// The colour a band gets from where it sits, not from what it is called.
///
/// Positional because band names come from the reference tables and a
/// profile is free to word them its own way — `low` here is
/// `deficiente` in another, and a match on either would go silently
/// colourless the day someone adds a table.
fn band_style(theme: &Theme, index: usize, count: usize) -> Style {
    if count <= 1 {
        return theme.muted();
    }
    match index {
        0 => theme.error(),
        index if index + 1 == count => theme.ok(),
        _ => theme.warn(),
    }
}

/// Why a dose is the size it is.
///
/// Efficiency divides the requirement, so it moves what somebody buys more
/// than any other figure in the plan — and the plan showed it as a bare
/// percentage. [`AdjustedEfficiency`] has carried the whole derivation all
/// along: the base range, every site condition that moved it, and the
/// bounds it was held inside. Its own documentation says the modifier list
/// is *"what makes a number in a report explainable"*, and no report was
/// reading it.
///
/// The modifiers are multiplicative, so each bar is drawn from the value
/// the one above it left: the shape **is** the arithmetic rather than an
/// illustration of it.
///
/// # Arguments
/// * `theme` — the palette.
/// * `efficiency` — the nutrient's efficiency, start to finish.
/// * `width` — cells the widest bar may take.
///
/// # Returns
/// One line per step: the base, one per modifier, and the result with the
/// floor and ceiling it was clamped against.
pub fn efficiency_waterfall<'a>(
    theme: &Theme,
    efficiency: &AdjustedEfficiency,
    width: usize,
) -> Vec<Line<'a>> {
    // Every bar is scaled against the ceiling rather than against the base,
    // so a clamp that lifted the result shows as a bar longer than the one
    // above it instead of overflowing the track.
    let scale = efficiency.ceiling.max(efficiency.base).max(efficiency.adjusted);
    let bar = |value: f64| -> usize {
        if scale > 0.0 {
            ((value / scale) * width as f64).round().clamp(0.0, width as f64) as usize
        } else {
            0
        }
    };
    let pct = |value: f64| format!("{:>4.0}%", value * 100.0);

    // A base with nothing acting on it is drawn once. Two identical bars
    // one above the other read as a step that did something, and the whole
    // point of the shape is that a step you can see is a step that
    // happened — the reason nothing moved is in `assumptions`, which the
    // caller prints under this.
    let mut lines = Vec::new();
    if !efficiency.modifiers.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("█".repeat(bar(efficiency.base)), theme.muted()),
            Span::styled(format!("  {}", pct(efficiency.base)), theme.base()),
        ]));
    }

    let mut running = efficiency.base;
    for modifier in &efficiency.modifiers {
        running *= modifier.factor;
        lines.push(Line::from(vec![
            Span::styled("█".repeat(bar(running)), theme.warn()),
            Span::styled(format!("  {}", pct(running)), theme.base()),
            Span::styled(
                format!("  ×{:.2}  {} · {}", modifier.factor, modifier.condition, modifier.effect),
                theme.muted(),
            ),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled("█".repeat(bar(efficiency.adjusted)), theme.accent()),
        Span::styled(format!("  {}", pct(efficiency.adjusted)), theme.strong()),
        Span::styled(
            format!("  ⌊{:.0} · {:.0}⌉", efficiency.floor * 100.0, efficiency.ceiling * 100.0),
            if efficiency.clamped { theme.warn() } else { theme.muted() },
        ),
    ]));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::EfficiencyModifier;
    use crate::infra::tui_adapter::theme::IMPERATOR;

    fn band(category: &str, min: Option<f64>, max: Option<f64>) -> QualitativeBand {
        QualitativeBand {
            property: "ph".to_string(),
            category: category.to_string(),
            min_value: min,
            max_value: max,
            unit: "ph".to_string(),
            source: "test".to_string(),
            year: 2009,
        }
    }

    fn painted(line: &Line) -> String {
        line.spans.iter().map(|span| span.content.as_ref()).collect()
    }

    /// The one thing a gauge must never do: paint the mark over a band the
    /// domain did not put the reading in. A drawing that disagrees with
    /// the classifier is worse than no drawing, because it is read faster.
    ///
    /// The bands here are the shape every real table has — open at both
    /// ends — which is exactly the shape that broke a linear track.
    #[test]
    fn the_mark_lands_in_the_band_the_classifier_chose() {
        let bands = [
            band("acid", None, Some(5.5)),
            band("slightly acid", Some(5.5), Some(6.5)),
            band("neutral", Some(6.5), None),
        ];

        for (value, expected) in [(4.0, "acid"), (5.4, "acid"), (6.0, "slightly acid"), (9.0, "neutral")] {
            let line = gauge(&IMPERATOR, value, &bands, 26);
            let cells: Vec<_> = line.spans[1..line.spans.len() - 1].to_vec();
            let mark = cells.iter().position(|span| span.content == MARK.to_string()).expect("a mark");
            let want = bands.iter().position(|b| b.category == expected).expect("the band");
            let share = cells.len() / bands.len();
            assert_eq!(mark / share, want, "{value} was drawn outside `{expected}`");
        }
    }

    /// Every gauge is exactly the width it was given, whatever the table
    /// behind it divides into. Otherwise a column of them ends in
    /// different places and whatever is printed after them goes ragged —
    /// which is the failure the tables on these pages were rebuilt to
    /// stop.
    #[test]
    fn a_gauge_is_the_width_it_was_asked_for() {
        let three = [band("a", None, Some(1.0)), band("b", Some(1.0), Some(2.0)), band("c", Some(2.0), None)];
        let mut nine = three.to_vec();
        nine.extend(three.iter().cloned());
        nine.extend(three.iter().cloned());

        for width in [12, 20, 26, 27, 31] {
            for bands in [three.as_slice(), nine.as_slice()] {
                let drawn = painted(&gauge(&IMPERATOR, 1.5, bands, width)).chars().count();
                assert_eq!(drawn, width, "{} bands at width {width} drew {drawn}", bands.len());
            }
        }
    }

    /// A value the table names no band for is unclassified, and the
    /// screen prints it as such. Drawing a mark anyway would assert a
    /// verdict the domain refused to give — see `QualitativeBand`, whose
    /// gaps are deliberate.
    #[test]
    fn a_reading_no_band_contains_gets_no_mark() {
        let bands = [band("ideal", Some(3.0), Some(5.0)), band("mg deficient", Some(10.0), None)];
        assert!(!painted(&gauge(&IMPERATOR, 7.0, &bands, 26)).contains(MARK), "7.0 is in the table's gap");
        assert!(painted(&gauge(&IMPERATOR, 4.0, &bands, 26)).contains(MARK), "4.0 is not");
    }

    /// The whole reason the gauge exists: two readings that share a
    /// verdict are not the same soil. `low` is open at the bottom in every
    /// real table, so without [`extent`] borrowing its neighbour's width
    /// these two would draw on the same cell.
    #[test]
    fn two_readings_in_one_open_band_do_not_draw_alike() {
        let level = crate::core::domain::CriticalLevel {
            low_threshold: 0.4,
            medium_threshold: 0.6,
            high_threshold: 0.6,
            unit: "cmolc_per_kg".to_string(),
            extraction_method: "any".to_string(),
            source: "test".to_string(),
            year: 2009,
        };
        let bands = level.bands("K");
        let mark_at = |value: f64| {
            painted(&gauge(&IMPERATOR, value, &bands, 26)).chars().position(|c| c == MARK).expect("a mark")
        };

        assert!(mark_at(0.39) > mark_at(0.05), "a reading at the edge of `low` must not draw like one at the floor");
        assert_eq!(level.classify(0.39), level.classify(0.05), "and both are still `low` — the word is what cannot tell them apart");
    }

    /// Nothing to measure against, or no room to measure in, draws
    /// nothing — an empty track would suggest a scale that does not exist.
    #[test]
    fn a_gauge_with_nothing_to_measure_against_draws_nothing() {
        assert!(painted(&gauge(&IMPERATOR, 6.0, &[], 26)).is_empty());
        assert!(painted(&gauge(&IMPERATOR, 6.0, &[band("a", Some(1.0), Some(2.0))], 3)).is_empty());
    }

    /// The waterfall's last bar is the number the plan actually divided by.
    /// If the drawing and `adjusted` ever part company, the picture is
    /// explaining a dose nobody was given.
    #[test]
    fn the_waterfall_ends_on_the_efficiency_the_plan_used() {
        let efficiency = AdjustedEfficiency {
            nutrient: crate::core::domain::Nutrient::N,
            base: 0.40,
            modifiers: vec![
                EfficiencyModifier {
                    factor: 0.85,
                    condition: "pH 5.3".to_string(),
                    effect: "volatilization".to_string(),
                    basis: "Havlin 2014".to_string(),
                },
                EfficiencyModifier {
                    factor: 0.90,
                    condition: "sandy loam".to_string(),
                    effect: "leaching".to_string(),
                    basis: "Barber 1995".to_string(),
                },
            ],
            adjusted: 0.306,
            floor: 0.25,
            ceiling: 0.60,
            clamped: false,
            assumptions: Vec::new(),
        };

        let lines = efficiency_waterfall(&IMPERATOR, &efficiency, 40);
        assert_eq!(lines.len(), 4, "the base, one row per modifier, and the result");
        let last = painted(&lines[3]);
        assert!(last.contains("31%"), "the result row must carry `adjusted`, not the base: {last}");
        assert!(painted(&lines[0]).contains("40%"), "and the first row the base");
        // Each modifier shrinks what the one above it left, which is what
        // makes the shape the arithmetic rather than a picture of it.
        let bars: Vec<usize> = lines.iter().map(|line| painted(line).matches('█').count()).collect();
        assert!(bars[0] > bars[1] && bars[1] > bars[2], "penalties must read as shrinking: {bars:?}");
    }
}
