//! Lays out the `ledger` variant: the splits as a table of kilometre numbers
//! and paces, each pace led by a swatch in its kilometre's colour.

use minijinja::{context, Value};
use serde::Serialize;

use crate::card::CONTENT_WIDTH;
use crate::format;
use crate::model::{Analysis, Meta, SplitView};
use crate::palette::Palette;
use crate::render::px;
use crate::text::{Role, TextMeasure};

/// Left edge of the table, the content box's.
const LEFT: f32 = 48.0;
/// Top of the visualisation zone, where the first row starts.
const ZONE_TOP: f32 = 200.0;
/// Height of the visualisation zone, which the longest column never leaves:
/// it ends at y 724 at most.
const ZONE_HEIGHT: f32 = 524.0;
/// The tallest row: a short column keeps it and leaves the zone's rest empty.
const MAX_ROW: f32 = 36.0;
/// A column takes this many rows before the table gets another column.
const ROWS_PER_COLUMN: usize = 14;
/// The most columns the table is spread over.
const MAX_COLUMNS: usize = 4;
/// Space between two columns.
const GUTTER: f32 = 24.0;
/// The figures are JetBrains Mono at 13 px, 0.73 em tall.
const FIGURE_SIZE: f32 = 13.0;
const FIGURE_HEIGHT: f32 = 0.73 * FIGURE_SIZE;
/// Space between the partial kilometre's number and its distance.
const NOTE_GAP: f32 = 8.0;
/// Thickness of the rule under a row.
const RULE: f32 = 1.0;
/// Side of the square swatch left of a pace.
const SWATCH: f32 = 10.0;
/// Space between the swatch and the pace's first figure.
const SWATCH_GAP: f32 = 12.0;
/// Height of the swatch's top above the baseline, centring it on the figures.
const SWATCH_RISE: f32 = 9.0;
/// Opacity of the partial kilometre's swatch.
const PARTIAL_OPACITY: f32 = 0.6;

/// The distance of the partial kilometre, set after its number.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Note {
    /// E.g. `0.46 km`.
    pub text: String,
    /// Left end.
    pub x: f32,
}

/// The 10 px square in the kilometre's colour left of its pace.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Swatch {
    /// Left edge, 22 px before the pace's first figure.
    pub x: f32,
    /// Top edge, 9 px above the baseline.
    pub y: f32,
    /// `Some(0.6)` for the partial kilometre, `None` for a full one.
    pub opacity: Option<f32>,
}

/// One kilometre as a row of the table.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    /// 1-based kilometre number, set left-aligned at `x`.
    pub index: u32,
    /// Left edge of the row's column.
    pub x: f32,
    /// Right edge of the row's column, where the pace ends.
    pub right: f32,
    /// Baseline of the figures.
    pub baseline: f32,
    /// Pace without the unit, e.g. `5:02`, set in `ink` unless the row is
    /// the fastest.
    pub pace: String,
    /// CSS colour of the kilometre, filling its swatch.
    pub color: String,
    /// The fastest full kilometre: its number and pace in the marker colour,
    /// the pace bold.
    pub fastest: bool,
    /// The square in the kilometre's colour before the pace.
    pub swatch: Swatch,
    /// The partial kilometre's distance, `None` for a full one.
    pub note: Option<Note>,
}

/// A 1 px hairline under a row.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Rule {
    pub x: f32,
    pub y: f32,
    pub width: f32,
}

/// What `templates/ledger.svg` draws inside the visualisation zone.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ledger {
    /// Number of columns the rows are spread over.
    pub columns: usize,
    /// Height of every row.
    pub row_height: f32,
    /// One row per split, down the first column, then down the next.
    pub rows: Vec<Row>,
    /// The hairlines under every row but the last of each column.
    pub rules: Vec<Rule>,
}

/// Lays out the ledger in card pixels.
///
/// The splits run down the first column, then down the next: one column up
/// to 14 splits, two up to 28, three up to 42, else four, sharing the 624 px
/// of the content box with 24 px gutters between them. Rows are 36 px, less
/// only when the longest column would not fit the zone. Every pace ends at
/// its column's right edge, a 10 px swatch 12 px before its first figure.
/// The fastest full kilometre's row is marked.
pub fn build(
    analysis: &Analysis,
    splits: &[SplitView],
    _palette: &Palette,
    measure: &TextMeasure,
    _meta: &Meta,
) -> Ledger {
    let n = splits.len();
    let columns = columns(n);
    let per_column = n.div_ceil(columns).max(1);
    let row_height = row_height(n);
    let width = (CONTENT_WIDTH - GUTTER * (columns - 1) as f32) / columns as f32;
    let mut rows = Vec::with_capacity(n);
    let mut rules = Vec::new();
    for (i, (view, split)) in splits.iter().zip(&analysis.splits).enumerate() {
        let (column, line) = (i / per_column, i % per_column);
        let x = LEFT + (width + GUTTER) * column as f32;
        let top = ZONE_TOP + row_height * line as f32;
        let last_of_column = line + 1 == per_column || i + 1 == n;
        if !last_of_column {
            rules.push(Rule {
                x,
                y: top + row_height - RULE,
                width,
            });
        }
        let number = measure.width(Role::Mono, &view.index.to_string(), FIGURE_SIZE);
        let pace = measure.width(Role::Mono, &view.pace, FIGURE_SIZE);
        let right = x + width;
        let baseline = top + row_height / 2.0 + FIGURE_HEIGHT / 2.0;
        rows.push(Row {
            index: view.index,
            x,
            right,
            baseline,
            pace: view.pace.clone(),
            color: view.color.clone(),
            fastest: analysis.fastest == Some(view.index),
            swatch: Swatch {
                x: right - pace - SWATCH_GAP - SWATCH,
                y: baseline - SWATCH_RISE,
                opacity: view.partial.then_some(PARTIAL_OPACITY),
            },
            note: view.partial.then(|| Note {
                text: format::distance(split.distance_m),
                x: x + number + NOTE_GAP,
            }),
        });
    }
    Ledger {
        columns,
        row_height,
        rows,
        rules,
    }
}

/// How many columns `n` rows are set in: one per 14 rows, at most four.
fn columns(n: usize) -> usize {
    n.div_ceil(ROWS_PER_COLUMN).clamp(1, MAX_COLUMNS)
}

/// Height of a row when `n` rows are set in [`columns`]`(n)` columns: 36 px,
/// or the zone's share of a row of the longest column.
fn row_height(n: usize) -> f32 {
    let per_column = n.div_ceil(columns(n)).max(1);
    MAX_ROW.min(ZONE_HEIGHT / per_column as f32)
}

impl Ledger {
    /// The block as the template sees it, under the key `ledger`, every
    /// number rounded to two decimals.
    pub fn to_value(&self) -> Value {
        let rows: Vec<Value> = self
            .rows
            .iter()
            .map(|row| {
                context! {
                    index => row.index,
                    x => px(row.x),
                    right => px(row.right),
                    baseline => px(row.baseline),
                    pace => &row.pace,
                    color => &row.color,
                    fastest => row.fastest,
                    swatch => context! {
                        x => px(row.swatch.x),
                        y => px(row.swatch.y),
                        opacity => row.swatch.opacity.map(px),
                    },
                    note => row.note.as_ref().map(|note| context! {
                        text => &note.text,
                        x => px(note.x),
                    }),
                }
            })
            .collect();
        let rules: Vec<Value> = self
            .rules
            .iter()
            .map(|rule| {
                context! {
                    x => px(rule.x),
                    y => px(rule.y),
                    width => px(rule.width),
                }
            })
            .collect();
        context! {
            rows => rows,
            rules => rules,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card;
    use crate::model::{CardData, Split};
    use crate::text::Role;
    use crate::theme::Theme;
    use crate::variant::Variant;
    use time::macros::date;

    fn split(index: u32, distance_m: f64, seconds: f64) -> Split {
        Split {
            index,
            distance_m,
            seconds,
            partial: distance_m < 1000.0,
        }
    }

    /// The fastest full split is the one with the smallest pace.
    fn analysis(splits: Vec<Split>) -> Analysis {
        let fastest = splits
            .iter()
            .filter(|s| !s.partial)
            .min_by(|a, b| a.pace_s_per_km().total_cmp(&b.pace_s_per_km()))
            .map(|s| s.index);
        Analysis {
            distance_m: splits.iter().map(|s| s.distance_m).sum(),
            elapsed_s: splits.iter().map(|s| s.seconds).sum(),
            fastest,
            start: None,
            points: Vec::new(),
            splits,
        }
    }

    /// `full` kilometres at paces from 5:00 to 5:51, km 4 the fastest at
    /// 4:40, then the partial of `partial_m` metres, if any, at 5:20.
    fn run(full: u32, partial_m: f64) -> Analysis {
        let mut splits: Vec<Split> = (1..=full)
            .map(|km| {
                let seconds = if km == 4 {
                    280.0
                } else {
                    300.0 + f64::from(km * 7 % 52)
                };
                split(km, 1000.0, seconds)
            })
            .collect();
        if partial_m > 0.0 {
            splits.push(split(full + 1, partial_m, partial_m * 0.32));
        }
        analysis(splits)
    }

    fn card_of(analysis: &Analysis) -> CardData {
        let meta = Meta {
            runner: "Mara Lindqvist".into(),
            event: "Harbour Half".into(),
            city: Some("Lisbon".into()),
            date: date!(2026 - 04 - 19),
        };
        card::build(
            analysis,
            &meta,
            &Palette::for_theme(Theme::Light),
            &TextMeasure::new(),
            Variant::Ledger,
            Theme::Light,
        )
    }

    fn ledger_of(analysis: &Analysis) -> Ledger {
        card_of(analysis).ledger.expect("the ledger block")
    }

    fn assert_near(actual: f32, expected: f32, what: &str) {
        assert!(
            (actual - expected).abs() < 1e-3,
            "{what} was {actual}, expected {expected}"
        );
    }

    #[test]
    fn a_column_takes_up_to_14_rows_and_there_are_at_most_four() {
        for (n, expected) in [
            (1, 1),
            (14, 1),
            (15, 2),
            (28, 2),
            (29, 3),
            (42, 3),
            (43, 4),
            (100, 4),
        ] {
            assert_eq!(columns(n), expected, "{n} splits");
        }
    }

    #[test]
    fn rows_are_36_px_unless_the_longest_column_needs_less_to_fit_the_zone() {
        // Up to 56 splits in four columns of 14 rows of 36 px.
        for n in [1, 10, 14, 22, 43, 56] {
            assert_near(row_height(n), 36.0, &format!("{n} splits"));
        }
        // 57 splits in four columns of 15 rows; 100 in four of 25.
        assert_near(row_height(57), 524.0 / 15.0, "57 splits");
        assert_near(row_height(100), 20.96, "100 splits");
    }

    #[test]
    fn every_row_and_rule_stays_inside_the_zone() {
        for n in [1, 14, 22, 43, 56, 57, 100] {
            let ledger = ledger_of(&run(n, 0.0));
            let per_column = n.div_ceil(ledger.columns as u32) as f32;
            assert!(per_column * ledger.row_height <= 524.0 + 1e-3, "{n} splits");
            for row in &ledger.rows {
                let what = format!("km {} of {n}", row.index);
                let top = row.baseline - ledger.row_height / 2.0 - 0.73 * 13.0 / 2.0;
                assert!(top >= 200.0 - 1e-3, "{what}: row from y {top}");
                let bottom = top + ledger.row_height;
                assert!(bottom <= 724.0 + 1e-3, "{what}: row to y {bottom}");
                assert!(row.x >= 48.0 && row.right <= 672.0 + 1e-3, "{what}");
            }
            for rule in &ledger.rules {
                assert!(
                    (200.0..724.0).contains(&rule.y),
                    "{n} splits: rule at {}",
                    rule.y
                );
            }
        }
    }

    #[test]
    fn a_half_is_two_columns_of_eleven_rows_filled_top_to_bottom() {
        let card = card_of(&run(21, 100.0));
        let ledger = card.ledger.expect("the ledger block");
        assert_eq!(ledger.columns, 2);
        assert_near(ledger.row_height, 36.0, "row height");
        assert_eq!(ledger.rows.len(), 22);
        for (i, (row, view)) in ledger.rows.iter().zip(&card.splits).enumerate() {
            let what = format!("km {}", view.index);
            let (column, line) = (i / 11, i % 11);
            // Two columns of 300 px with a 24 px gutter between them.
            assert_near(row.x, 48.0 + 324.0 * column as f32, &what);
            assert_near(row.right, row.x + 300.0, &what);
            // Figures 0.73 em tall, centred on the row.
            let centre = 200.0 + 36.0 * line as f32 + 18.0;
            assert_near(row.baseline, centre + 0.73 * 13.0 / 2.0, &what);
            assert_eq!(row.index, view.index, "{what}");
            assert_eq!(row.pace, view.pace, "{what}");
            assert_eq!(row.color, view.color, "{what}");
        }
        assert_near(ledger.rows[21].right, 672.0, "the right edge of the table");
    }

    #[test]
    fn every_row_but_the_last_of_its_column_has_a_rule_under_it() {
        let ledger = ledger_of(&run(21, 100.0));
        assert_eq!(ledger.rules.len(), 20);
        for (i, rule) in ledger.rules.iter().enumerate() {
            let (column, line) = (i / 10, i % 10);
            let what = format!("rule {i}");
            assert_near(rule.x, 48.0 + 324.0 * column as f32, &what);
            assert_near(rule.width, 300.0, &what);
            assert_near(rule.y, 200.0 + 36.0 * (line + 1) as f32 - 1.0, &what);
        }
    }

    #[test]
    fn a_short_last_column_has_no_rule_under_its_last_row() {
        // 30 splits: three columns of 10, 10 and 10 rows; 31: 11, 11 and 9.
        assert_eq!(ledger_of(&run(30, 0.0)).rules.len(), 27);
        assert_eq!(ledger_of(&run(31, 0.0)).rules.len(), 28);
    }

    #[test]
    fn the_columns_of_a_marathon_are_138_px_wide_and_fill_the_content_box() {
        let ledger = ledger_of(&run(42, 195.0));
        assert_eq!(ledger.columns, 4);
        let lefts: Vec<f32> = ledger.rows.iter().step_by(11).map(|row| row.x).collect();
        assert_eq!(lefts, [48.0, 210.0, 372.0, 534.0]);
        assert!(ledger.rows.iter().all(|row| row.right - row.x == 138.0));
    }

    #[test]
    fn only_the_fastest_kilometre_row_is_marked() {
        let ledger = ledger_of(&run(10, 460.0));
        assert_eq!(ledger.rows.len(), 11);
        for row in &ledger.rows {
            assert_eq!(row.fastest, row.index == 4, "km {}", row.index);
        }
    }

    #[test]
    fn the_partial_kilometre_row_notes_its_distance_after_its_number() {
        let ledger = ledger_of(&run(10, 460.0));
        let partial = &ledger.rows[10];
        let number = TextMeasure::new().width(Role::Mono, "11", 13.0);
        assert_eq!(
            partial.note,
            Some(Note {
                text: "0.46 km".into(),
                x: partial.x + number + 8.0,
            })
        );
        assert!(ledger.rows[..10].iter().all(|row| row.note.is_none()));
    }

    #[test]
    fn a_swatch_in_the_kilometre_colour_sits_12_px_before_the_first_figure_of_each_pace() {
        let card = card_of(&run(21, 100.0));
        let ledger = card.ledger.expect("the ledger block");
        let measure = TextMeasure::new();
        for (row, view) in ledger.rows.iter().zip(&card.splits) {
            let what = format!("km {}", view.index);
            let pace = measure.width(Role::Mono, &view.pace, 13.0);
            // 10 px wide, its right edge 12 px left of the right-aligned pace.
            assert_near(row.swatch.x, row.right - pace - 12.0 - 10.0, &what);
            // 10 px tall, centred on the figures.
            assert_near(row.swatch.y, row.baseline - 9.0, &what);
            assert_eq!(row.color, view.color, "{what}");
        }
    }

    #[test]
    fn a_longer_pace_moves_its_swatch_left_by_its_extra_figure() {
        let ledger = ledger_of(&analysis(vec![
            split(1, 1000.0, 300.0),
            split(2, 1000.0, 605.0),
        ]));
        let (short, long) = (&ledger.rows[0], &ledger.rows[1]);
        assert_eq!((short.pace.as_str(), long.pace.as_str()), ("5:00", "10:05"));
        let figure = TextMeasure::new().width(Role::Mono, "1", 13.0);
        assert!(figure > 0.0);
        assert_near(long.swatch.x, short.swatch.x - figure, "10:05");
    }

    #[test]
    fn only_the_partial_kilometre_swatch_is_at_0_6_opacity() {
        let ledger = ledger_of(&run(10, 460.0));
        assert_eq!(ledger.rows[10].swatch.opacity, Some(0.6));
        assert!(ledger.rows[..10]
            .iter()
            .all(|row| row.swatch.opacity.is_none()));
    }

    #[test]
    fn the_template_sets_the_paces_in_ink_or_marker_and_only_the_swatch_in_the_kilometre_colour() {
        let template = include_str!("../../templates/ledger.svg");
        let line_of = |text: &str| {
            template
                .lines()
                .find(|line| line.contains(text))
                .unwrap_or_else(|| panic!("{text} in {template}"))
        };
        let pace = line_of("{{ row.pace }}");
        let fill = r#"fill="{{ theme.marker if row.fastest else theme.ink }}""#;
        assert!(pace.contains(fill), "{pace}");
        assert!(pace.contains("{% if row.fastest %}"), "{pace}");
        let number = line_of("{{ row.index }}");
        let fill = r#"fill="{{ theme.marker if row.fastest else theme.muted }}""#;
        assert!(number.contains(fill), "{number}");
        let swatch = template
            .lines()
            .find(|line| line.contains(r#"width="10" height="10" rx="3""#))
            .expect("the swatch");
        assert!(swatch.contains(r#"fill="{{ row.color }}""#), "{swatch}");
        assert_eq!(template.matches("row.color").count(), 1, "{template}");
    }

    #[test]
    fn a_single_split_is_a_single_row_without_a_rule() {
        let ledger = ledger_of(&analysis(vec![split(1, 600.0, 180.0)]));
        assert_eq!(ledger.columns, 1);
        assert_eq!(ledger.rows.len(), 1);
        assert!(ledger.rules.is_empty());
        let row = &ledger.rows[0];
        assert_eq!((row.pace.as_str(), row.fastest), ("5:00", false));
        assert_near(row.right, 672.0, "right edge");
    }

    #[test]
    fn the_template_value_has_every_number_rounded_to_two_decimals() {
        // Four columns of 25 rows of 20.96 px.
        let value = ledger_of(&run(99, 0.0)).to_value();
        let row = value
            .get_attr("rows")
            .unwrap()
            .get_item(&Value::from(0))
            .unwrap();
        let baseline = row.get_attr("baseline").unwrap().to_string();
        let decimals = baseline.split_once('.').map_or(0, |(_, d)| d.len());
        assert!(decimals <= 2, "baseline {baseline}");
        assert_eq!(row.get_attr("x").unwrap().to_string(), "48");
        assert_eq!(row.get_attr("right").unwrap().to_string(), "186");
        assert!(!row.get_attr("fastest").unwrap().is_true());
        let fourth = value
            .get_attr("rows")
            .unwrap()
            .get_item(&Value::from(3))
            .unwrap();
        assert!(fourth.get_attr("fastest").unwrap().is_true());
        assert_eq!(row.get_attr("index").unwrap().to_string(), "1");
        assert!(row.get_attr("note").unwrap().is_none());
        let swatch = row.get_attr("swatch").unwrap();
        for key in ["x", "y"] {
            let v = swatch.get_attr(key).unwrap().to_string();
            let decimals = v.split_once('.').map_or(0, |(_, d)| d.len());
            assert!(decimals <= 2, "swatch {key} {v}");
        }
        assert!(swatch.get_attr("opacity").unwrap().is_none());
        let rule = value
            .get_attr("rules")
            .unwrap()
            .get_item(&Value::from(0))
            .unwrap();
        assert_eq!(rule.get_attr("y").unwrap().to_string(), "219.96");
        assert_eq!(rule.get_attr("width").unwrap().to_string(), "138");
        let partial = ledger_of(&run(10, 460.0))
            .to_value()
            .get_attr("rows")
            .unwrap()
            .get_item(&Value::from(10))
            .unwrap();
        let opacity = partial.get_attr("swatch").unwrap().get_attr("opacity");
        assert_eq!(opacity.unwrap().to_string(), "0.6");
    }
}
