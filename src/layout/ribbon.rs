//! Lays out the `ribbon` variant: one horizontal bar per kilometre, longer for faster paces.

use minijinja::{context, Value};
use serde::Serialize;

use crate::model::{Analysis, Meta, SplitView};
use crate::palette::Palette;
use crate::render::px;
use crate::text::TextMeasure;

/// Top of the visualisation zone, where the first row starts.
const ZONE_TOP: f32 = 160.0;
/// Height of the visualisation zone, which the rows never leave.
const ZONE_HEIGHT: f32 = 560.0;
/// The tallest row: a short run keeps it and leaves the rest of the zone empty.
const MAX_ROW: f32 = 18.0;
/// Space between rows, smaller only when rows would get thinner than
/// [`MIN_ROW`].
const GAP: f32 = 4.0;
/// The thinnest row a long run gets before its gaps shrink.
const MIN_ROW: f32 = 1.0;
/// Left end of every bar.
const BAR_X: f32 = 88.0;
/// Length of a bar at the slow anchor's pace or slower.
const MIN_LENGTH: f32 = 96.0;
/// What a bar gains from the slow to the fast anchor, so the fastest ends at x 672.
const LENGTH_RANGE: f32 = 488.0;
/// Right end of the kilometre numbers.
const LABEL_X: f32 = 76.0;
/// The kilometre numbers are JetBrains Mono at 11 px, its figures 0.73 em tall.
const LABEL_SIZE: f32 = 11.0;
const FIGURE_HEIGHT: f32 = 0.73 * LABEL_SIZE;
/// Up to this many rows every kilometre is numbered.
const ALL_NUMBERED: usize = 31;
/// Beyond [`ALL_NUMBERED`] rows only every this many kilometres (and the last) are.
const NUMBER_EVERY: u32 = 5;
/// The partial kilometre's bar is see-through.
const PARTIAL_OPACITY: f32 = 0.6;
/// The fastest bar's outline: a 2 px stroke centred 1 px inside its edge.
const OUTLINE_INSET: f32 = 1.0;

/// One kilometre as a rounded bar from x 88 to the right.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bar {
    /// 1-based kilometre number.
    pub index: u32,
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Length, `96 + 488 * t` for the palette position `t`.
    pub width: f32,
    pub height: f32,
    /// Corner radius, half the height: the ends are round.
    pub rx: f32,
    /// CSS colour of the kilometre's pace.
    pub color: String,
    /// 0.6 for the partial kilometre, `None` for a full one.
    pub opacity: Option<f32>,
    /// Baseline of the kilometre number, `None` when the row is not numbered.
    pub label_y: Option<f32>,
}

/// The ink outline drawn inside the fastest kilometre's bar.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Outline {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rx: f32,
}

/// What `templates/ribbon.svg` draws inside the visualisation zone.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ribbon {
    /// One bar per split, top to bottom.
    pub bars: Vec<Bar>,
    /// Right end of the kilometre numbers.
    pub label_x: f32,
    /// The outline of the fastest full kilometre's bar, if there is one.
    pub outline: Option<Outline>,
}

/// Lays out the ribbon in card pixels.
///
/// One row per split from the top of the zone, 18 px high with 4 px gaps
/// until the rows fill the zone, thinner beyond; rows are never stretched.
/// A bar's length follows its palette position, the partial kilometre's too.
pub fn build(
    analysis: &Analysis,
    splits: &[SplitView],
    _palette: &Palette,
    _measure: &TextMeasure,
    _meta: &Meta,
) -> Ribbon {
    let n = splits.len();
    let (height, gap) = (row_height(n), gap(n));
    let bars: Vec<Bar> = splits
        .iter()
        .enumerate()
        .map(|(row, view)| {
            let y = ZONE_TOP + row as f32 * (height + gap);
            Bar {
                index: view.index,
                x: BAR_X,
                y,
                width: bar_length(view.t),
                height,
                rx: height / 2.0,
                color: view.color.clone(),
                opacity: view.partial.then_some(PARTIAL_OPACITY),
                label_y: numbered(view.index, n).then_some(y + height / 2.0 + FIGURE_HEIGHT / 2.0),
            }
        })
        .collect();
    let outline = analysis
        .fastest
        .and_then(|index| bars.iter().find(|bar| bar.index == index))
        .map(|bar| Outline {
            x: bar.x + OUTLINE_INSET,
            y: bar.y + OUTLINE_INSET,
            width: bar.width - 2.0 * OUTLINE_INSET,
            height: bar.height - 2.0 * OUTLINE_INSET,
            rx: bar.rx,
        });
    Ribbon {
        bars,
        label_x: LABEL_X,
        outline,
    }
}

/// Height of each of `n` rows: 18 px, or what fills the zone with 4 px
/// gaps, but not below 1 px (or the zone's share of a row past 560 rows).
fn row_height(n: usize) -> f32 {
    let n = n.max(1) as f32;
    let filling = (ZONE_HEIGHT - GAP * (n - 1.0)) / n;
    MAX_ROW.min(filling).max(MIN_ROW.min(ZONE_HEIGHT / n))
}

/// Space between two of `n` rows: 4 px, less when the rows are at their
/// thinnest and still would not fit.
fn gap(n: usize) -> f32 {
    if n < 2 {
        return GAP;
    }
    let rows = n as f32 * row_height(n);
    GAP.min((ZONE_HEIGHT - rows) / (n - 1) as f32).max(0.0)
}

/// Length of a bar at palette position `t`, 0 at the slow anchor to 1 at the
/// fast one.
fn bar_length(t: f32) -> f32 {
    MIN_LENGTH + LENGTH_RANGE * t
}

/// Whether kilometre `index` of `n` gets its number: all of them up to 31
/// rows, else every fifth and the last.
fn numbered(index: u32, n: usize) -> bool {
    n <= ALL_NUMBERED || index.is_multiple_of(NUMBER_EVERY) || index as usize == n
}

impl Ribbon {
    /// The block as the template sees it, under the key `ribbon`, every
    /// number rounded to two decimals.
    pub fn to_value(&self) -> Value {
        let bars: Vec<Value> = self
            .bars
            .iter()
            .map(|bar| {
                context! {
                    index => bar.index,
                    x => px(bar.x),
                    y => px(bar.y),
                    width => px(bar.width),
                    height => px(bar.height),
                    rx => px(bar.rx),
                    color => &bar.color,
                    opacity => bar.opacity.map(px),
                    label_y => bar.label_y.map(px),
                }
            })
            .collect();
        context! {
            bars => bars,
            label_x => px(self.label_x),
            outline => self.outline.as_ref().map(|o| context! {
                x => px(o.x),
                y => px(o.y),
                width => px(o.width),
                height => px(o.height),
                rx => px(o.rx),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card;
    use crate::model::Split;
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

    /// `n` full kilometres at paces from 5:00 to 5:51, km 4 the fastest at 4:40.
    fn run(n: u32) -> Analysis {
        analysis(
            (1..=n)
                .map(|km| {
                    let seconds = if km == 4 {
                        280.0
                    } else {
                        300.0 + f64::from(km * 7 % 52)
                    };
                    split(km, 1000.0, seconds)
                })
                .collect(),
        )
    }

    /// The ribbon block and the split views of the card for `analysis`.
    fn ribbon_of(analysis: &Analysis) -> (Ribbon, Vec<SplitView>) {
        let meta = Meta {
            runner: "Mara Lindqvist".into(),
            event: "Harbour Half".into(),
            city: Some("Lisbon".into()),
            date: date!(2026 - 04 - 19),
        };
        let card = card::build(
            analysis,
            &meta,
            &Palette::for_theme(Theme::Light),
            &TextMeasure::new(),
            Variant::Ribbon,
            Theme::Light,
        );
        (card.ribbon.expect("the ribbon block"), card.splits)
    }

    fn assert_near(actual: f32, expected: f32, what: &str) {
        assert!(
            (actual - expected).abs() < 1e-3,
            "{what} was {actual}, expected {expected}"
        );
    }

    #[test]
    fn rows_are_18_px_until_they_and_their_4_px_gaps_fill_the_zone() {
        for n in [1, 10, 25] {
            assert_near(row_height(n), 18.0, &format!("{n} rows"));
            assert_near(gap(n), 4.0, &format!("gap of {n} rows"));
        }
        assert_near(row_height(26), 460.0 / 26.0, "26 rows");
        assert_near(row_height(42), 396.0 / 42.0, "42 rows");
        for n in [26, 42, 100] {
            let total = n as f32 * row_height(n) + (n - 1) as f32 * gap(n);
            assert_near(total, 560.0, &format!("{n} rows and their gaps"));
        }
    }

    #[test]
    fn past_112_rows_the_gaps_shrink_so_no_row_is_thinner_than_a_pixel() {
        for n in [113, 150, 300] {
            let (height, gap) = (row_height(n), gap(n));
            assert!(height >= 0.9, "{n} rows of {height} px");
            assert!((0.0..4.0).contains(&gap), "{n} rows with gaps of {gap} px");
            let total = n as f32 * height + (n - 1) as f32 * gap;
            assert!(total <= 560.0 + 1e-3, "{n} rows take {total} px");
        }
    }

    #[test]
    fn a_bar_is_96_px_at_the_slow_anchor_and_reaches_x_672_at_the_fast_one() {
        assert_near(bar_length(0.0), 96.0, "slow anchor");
        assert_near(bar_length(0.5), 340.0, "half way");
        assert_near(
            88.0 + bar_length(1.0),
            672.0,
            "right end at the fast anchor",
        );
    }

    #[test]
    fn a_ten_k_has_ten_rows_of_18_px_from_the_top_of_the_zone() {
        let (ribbon, views) = ribbon_of(&run(10));
        assert_eq!(ribbon.bars.len(), 10);
        for (i, (bar, view)) in ribbon.bars.iter().zip(&views).enumerate() {
            let what = format!("km {}", view.index);
            assert_eq!(bar.index, view.index);
            assert_near(bar.x, 88.0, &what);
            assert_near(bar.y, 160.0 + 22.0 * i as f32, &what);
            assert_near(bar.height, 18.0, &what);
            assert_near(bar.rx, 9.0, &what);
            assert_near(bar.width, 96.0 + 488.0 * view.t, &what);
            assert_eq!(bar.color, view.color, "{what}");
            assert_eq!(bar.opacity, None, "{what}");
        }
        assert_near(ribbon.bars[3].width, 584.0, "the fastest km reaches x 672");
    }

    #[test]
    fn the_numbers_are_right_aligned_at_x_76_and_centred_on_their_rows() {
        let (ribbon, _) = ribbon_of(&run(10));
        assert_near(ribbon.label_x, 76.0, "label x");
        for bar in &ribbon.bars {
            // JetBrains Mono's figures are 0.73 em tall.
            let centred = bar.y + bar.height / 2.0 + 0.73 * 11.0 / 2.0;
            let baseline = bar.label_y.expect("every row of a ten k is numbered");
            assert_near(baseline, centred, &format!("km {}", bar.index));
        }
    }

    #[test]
    fn the_partial_kilometre_has_its_own_length_and_is_translucent() {
        let mut splits = run(5).splits;
        splits.push(split(6, 460.0, 120.0));
        let (ribbon, views) = ribbon_of(&analysis(splits));
        let partial = &ribbon.bars[5];
        assert_eq!(partial.opacity, Some(0.6));
        assert_near(partial.width, 96.0 + 488.0 * views[5].t, "partial length");
        assert!(ribbon.bars[..5].iter().all(|bar| bar.opacity.is_none()));
    }

    #[test]
    fn the_fastest_bar_gets_an_outline_inset_by_one_pixel() {
        let (ribbon, _) = ribbon_of(&run(10));
        let fastest = &ribbon.bars[3];
        assert_eq!(
            ribbon.outline,
            Some(Outline {
                x: fastest.x + 1.0,
                y: fastest.y + 1.0,
                width: fastest.width - 2.0,
                height: fastest.height - 2.0,
                rx: fastest.rx,
            })
        );
    }

    #[test]
    fn without_a_full_kilometre_nothing_is_outlined() {
        let (ribbon, _) = ribbon_of(&analysis(vec![split(1, 600.0, 180.0)]));
        assert_eq!(ribbon.bars.len(), 1);
        assert_eq!(ribbon.outline, None);
    }

    #[test]
    fn up_to_31_rows_every_kilometre_is_numbered() {
        assert!((1..=31).all(|index| numbered(index, 31)));
        let (ribbon, _) = ribbon_of(&run(31));
        assert!(ribbon.bars.iter().all(|bar| bar.label_y.is_some()));
    }

    #[test]
    fn from_32_rows_every_fifth_and_the_last_kilometre_are_numbered() {
        let (ribbon, _) = ribbon_of(&run(43));
        let shown: Vec<u32> = ribbon
            .bars
            .iter()
            .filter(|bar| bar.label_y.is_some())
            .map(|bar| bar.index)
            .collect();
        assert_eq!(shown, [5, 10, 15, 20, 25, 30, 35, 40, 43]);
        assert!(numbered(40, 40) && numbered(32, 32) && !numbered(31, 32));
    }

    #[test]
    fn a_marathon_fills_the_zone_to_its_bottom_edge() {
        let (ribbon, _) = ribbon_of(&run(43));
        let last = ribbon.bars.last().expect("43 bars");
        assert_near(last.y + last.height, 720.0, "bottom of the last bar");
    }

    #[test]
    fn the_template_value_has_every_number_rounded_to_two_decimals() {
        let (ribbon, _) = ribbon_of(&run(26));
        let value = ribbon.to_value();
        let bar = value
            .get_attr("bars")
            .unwrap()
            .get_item(&Value::from(1))
            .unwrap();
        // 460 / 26 = 17.6923…, the second row 4 px below the first.
        assert_eq!(bar.get_attr("height").unwrap().to_string(), "17.69");
        assert_eq!(bar.get_attr("y").unwrap().to_string(), "181.69");
        assert_eq!(bar.get_attr("x").unwrap().to_string(), "88");
        assert_eq!(bar.get_attr("index").unwrap().to_string(), "2");
        assert!(bar.get_attr("opacity").unwrap().is_none());
        assert_eq!(value.get_attr("label_x").unwrap().to_string(), "76");
        let outline = value.get_attr("outline").unwrap();
        assert_eq!(outline.get_attr("x").unwrap().to_string(), "89");
    }
}
