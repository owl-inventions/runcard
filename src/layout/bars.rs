//! Lays out the `bars` variant: a speed chart with one bar per kilometre.

use minijinja::{context, Value};
use serde::Serialize;

use crate::card;
use crate::format;
use crate::model::{Analysis, Meta, Split, SplitView};
use crate::palette::{self, Palette};
use crate::render::px;
use crate::text::{Font, TextMeasure};

/// The baseline the bars stand on.
const BASELINE: f32 = 560.0;
/// Top of the chart, which a bar at the fast anchor touches.
const CHART_TOP: f32 = 190.0;
/// Height of a bar at the fast anchor's pace or faster.
const MAX_HEIGHT: f32 = BASELINE - CHART_TOP;
/// Height of a bar at the slow anchor's pace or slower.
const MIN_HEIGHT: f32 = 80.0;
/// Gap between the fastest bar's top and the centre of its dot.
const DOT_ABOVE: f32 = 12.0;
/// Gap between the average line and the baseline of its label above it.
const LABEL_ABOVE: f32 = 6.0;
/// Gap between the average line and the baseline of its label below it.
const LABEL_BELOW: f32 = 14.0;
/// An average line closer than this to the top has its label below it.
const LABEL_ROOM: f32 = 16.0;
/// The label's right end, the right edge of the chart.
const LABEL_RIGHT: f32 = 688.0;
/// The label is IBM Plex Mono at 12 px, its figures 0.7 em tall.
const LABEL_SIZE: f32 = 12.0;
const LABEL_FIGURES: f32 = 0.7 * LABEL_SIZE;
/// Room between the label and the edge of its plate.
const PLATE_PADDING: f32 = 3.0;

/// One kilometre as a bar standing on the baseline.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bar {
    /// 1-based kilometre number.
    pub index: u32,
    /// Left edge, the same as the kilometre's classic stripe.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// CSS colour of the kilometre's pace.
    pub color: String,
}

/// What `templates/bars.svg` draws besides the shared card data.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bars {
    /// One bar per split, in order, `80 + 290 * t` px high for its palette
    /// position `t`: 80 px at the slow anchor, 370 px at the fast one.
    pub bars: Vec<Bar>,
    /// Height of the dashed average line: where a bar at the average pace's
    /// palette position would end.
    pub avg_y: f32,
    /// The average line's label, e.g. `AVG 5:43`.
    pub avg_label: String,
    /// Baseline of the label, just above the line or, near the top, below it.
    pub avg_label_y: f32,
    /// Paper plate behind the label, so a bar running through it does not
    /// cross the text: x, y, width and height.
    pub avg_label_plate: (f32, f32, f32, f32),
    /// Centre of the dot above the fastest full kilometre's bar, if there is one.
    pub fastest_dot: Option<(f32, f32)>,
}

/// Lays out the bar chart in card pixels.
///
/// A bar's height follows its palette position, linear from 80 px at the
/// slow anchor to the full 370 px at the fast anchor, so the chart spans its
/// whole range on any run; the trailing partial stands at its own position.
pub fn build(
    analysis: &Analysis,
    splits: &[SplitView],
    _palette: &Palette,
    measure: &TextMeasure,
    _meta: &Meta,
) -> Bars {
    let average = analysis.elapsed_s / analysis.distance_m * 1000.0;
    let spans = card::spans(&analysis.splits);
    let bars: Vec<Bar> = splits
        .iter()
        .zip(spans)
        .map(|(view, span)| {
            let height = height(view.t);
            Bar {
                index: view.index,
                x: span.x,
                y: BASELINE - height,
                width: span.width,
                height,
                color: view.color.clone(),
            }
        })
        .collect();
    let avg_y = BASELINE - height(average_position(&analysis.splits, average));
    let fastest_dot = analysis
        .fastest
        .and_then(|index| bars.iter().find(|bar| bar.index == index))
        .map(|bar| (bar.x + bar.width / 2.0, bar.y - DOT_ABOVE));
    let avg_label = format!("AVG {}", format::pace_short(average));
    let avg_label_y = label_y(avg_y);
    let label_width = measure.width(Font::Mono, &avg_label, LABEL_SIZE, 0.0);
    Bars {
        bars,
        avg_y,
        avg_label,
        avg_label_y,
        avg_label_plate: (
            LABEL_RIGHT - label_width - PLATE_PADDING,
            avg_label_y - LABEL_FIGURES - PLATE_PADDING,
            label_width + 2.0 * PLATE_PADDING,
            LABEL_FIGURES + 2.0 * PLATE_PADDING,
        ),
        fastest_dot,
    }
}

/// Height of a bar at palette position `t`, 0 at the slow anchor to 1 at the
/// fast one.
fn height(t: f32) -> f32 {
    MIN_HEIGHT + (MAX_HEIGHT - MIN_HEIGHT) * t
}

/// Palette position of the `average` pace between the anchors of the full
/// kilometres: `CardData::avg_t`, which the layout is not handed.
fn average_position(splits: &[Split], average: f64) -> f32 {
    let full: Vec<f64> = splits
        .iter()
        .filter(|s| !s.partial)
        .map(Split::pace_s_per_km)
        .collect();
    let (slow, fast) = palette::anchors(&full);
    palette::pace_position(average, slow, fast)
}

/// Baseline of the label of an average line at `avg_y`.
fn label_y(avg_y: f32) -> f32 {
    if avg_y < CHART_TOP + LABEL_ROOM {
        avg_y + LABEL_BELOW
    } else {
        avg_y - LABEL_ABOVE
    }
}

impl Bars {
    /// The block as the template's `bars` value, numbers rounded to two decimals.
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
                    color => &bar.color,
                }
            })
            .collect();
        context! {
            bars => bars,
            avg_y => px(self.avg_y),
            avg_label => &self.avg_label,
            avg_label_y => px(self.avg_label_y),
            avg_label_plate => {
                let (x, y, width, height) = self.avg_label_plate;
                Value::from(vec![px(x), px(y), px(width), px(height)])
            },
            fastest_dot => self.fastest_dot.map(|(x, y)| Value::from(vec![px(x), px(y)])),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card;
    use crate::model::{CardData, Split};
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

    fn card_of(analysis: &Analysis) -> CardData {
        let meta = Meta {
            runner: "Julia Brandt".into(),
            event: "Spreelauf".into(),
            city: None,
            date: date!(2026 - 09 - 27),
        };
        card::build(
            analysis,
            &meta,
            &Palette::light(),
            &TextMeasure::new(),
            Variant::Bars,
        )
    }

    fn bars_of(analysis: &Analysis) -> Bars {
        card_of(analysis).bars.expect("the bars block")
    }

    fn heights(bars: &Bars) -> Vec<f32> {
        bars.bars.iter().map(|b| b.height).collect()
    }

    fn assert_near(actual: f32, expected: f32, what: &str) {
        assert!(
            (actual - expected).abs() < 1e-3,
            "{what} was {actual}, expected {expected}"
        );
    }

    /// 300 s and 300 s for two kilometres, then 900 m in 225 s: an average
    /// of 4:44, faster than the fast anchor at 4:50.
    fn fast_finish() -> Analysis {
        analysis(vec![
            split(1, 1000.0, 300.0),
            split(2, 1000.0, 300.0),
            split(3, 900.0, 225.0),
        ])
    }

    #[test]
    fn a_bar_is_80_px_at_the_slow_anchor_370_at_the_fast_one_and_225_half_way() {
        assert_near(height(0.0), 80.0, "slow anchor");
        assert_near(height(1.0), 370.0, "fast anchor");
        assert_near(height(0.5), 225.0, "half way");
    }

    #[test]
    fn every_bar_is_as_high_as_its_palette_position_says() {
        let analysis = analysis(vec![
            split(1, 1000.0, 330.0),
            split(2, 1000.0, 300.0),
            split(3, 1000.0, 600.0),
            split(4, 1000.0, 345.0),
            split(5, 1000.0, 318.0),
            split(6, 460.0, 150.0),
        ]);
        let card = card_of(&analysis);
        let bars = card.bars.expect("the bars block");
        assert_eq!(bars.bars.len(), 6);
        for (bar, view) in bars.bars.iter().zip(&card.splits) {
            let what = format!("km {}", bar.index);
            assert_near(bar.height, 80.0 + 290.0 * view.t, &what);
            assert_near(bar.y, 560.0 - bar.height, &what);
        }
        assert_near(bars.bars[1].height, 370.0, "the fastest km at the top");
        assert_near(bars.bars[2].height, 80.0, "the slowest km at the bottom");
    }

    #[test]
    fn the_partial_kilometre_stands_at_its_own_palette_position() {
        let fast = bars_of(&fast_finish());
        assert_near(
            fast.bars[2].height,
            370.0,
            "partial faster than the fast anchor",
        );
        let slow = bars_of(&analysis(vec![
            split(1, 1000.0, 300.0),
            split(2, 1000.0, 300.0),
            split(3, 400.0, 160.0),
        ]));
        assert_near(
            slow.bars[2].height,
            80.0,
            "partial slower than the slow anchor",
        );
    }

    #[test]
    fn the_bars_stand_on_the_classic_stripes_in_the_kilometre_colours() {
        let analysis = analysis(vec![
            split(1, 1000.0, 300.0),
            split(2, 1000.0, 320.0),
            split(3, 460.0, 150.0),
        ]);
        let bars = bars_of(&analysis);
        let spans = card::spans(&analysis.splits);
        let palette = Palette::light();
        for ((bar, span), index) in bars.bars.iter().zip(&spans).zip(1..) {
            assert_eq!(bar.index, index);
            assert_near(bar.x, span.x, &format!("x of km {index}"));
            assert_near(bar.width, span.width, &format!("width of km {index}"));
            assert_near(bar.y + bar.height, 560.0, &format!("bottom of km {index}"));
        }
        assert_eq!(bars.bars[0].color, palette.hex(1.0), "km 1");
        assert_eq!(bars.bars[1].color, palette.hex(0.0), "km 2");
    }

    #[test]
    fn the_average_line_sits_at_the_average_palette_position_with_its_label_above() {
        // 300 s + 600 s over 2 km: an average of 7:30, half way between the
        // anchors at 5:15 and 9:45.
        let analysis = analysis(vec![split(1, 1000.0, 300.0), split(2, 1000.0, 600.0)]);
        let card = card_of(&analysis);
        let bars = card.bars.expect("the bars block");
        assert_near(card.avg_t, 0.5, "average position");
        assert_near(
            bars.avg_y,
            560.0 - (80.0 + 290.0 * card.avg_t),
            "average line",
        );
        assert_near(bars.avg_y, 335.0, "average line half way");
        assert_eq!(bars.avg_label, "AVG 7:30");
        assert_near(bars.avg_label_y, bars.avg_y - 6.0, "label");
    }

    #[test]
    fn the_average_line_follows_the_average_palette_position_of_any_run() {
        let runs = [
            analysis(vec![
                split(1, 1000.0, 330.0),
                split(2, 1000.0, 300.0),
                split(3, 1000.0, 600.0),
                split(4, 1000.0, 345.0),
                split(5, 460.0, 150.0),
            ]),
            fast_finish(),
            analysis(vec![split(1, 600.0, 180.0)]),
        ];
        for analysis in &runs {
            let card = card_of(analysis);
            let avg_y = card.bars.expect("the bars block").avg_y;
            assert_near(avg_y, 560.0 - (80.0 + 290.0 * card.avg_t), "average line");
        }
    }

    #[test]
    fn an_average_line_near_the_top_has_its_label_below() {
        let bars = bars_of(&fast_finish());
        assert_near(bars.avg_y, 190.0, "average line of a fast finish");
        assert_eq!(bars.avg_label, "AVG 4:44");
        assert_near(bars.avg_label_y, 204.0, "label below the line");
    }

    #[test]
    fn the_label_goes_below_once_the_line_is_within_16_px_of_the_top() {
        assert_near(label_y(206.0), 200.0, "label of a line 16 px down");
        assert_near(label_y(205.9), 219.9, "label of a line 15.9 px down");
        assert_near(label_y(400.0), 394.0, "label of a low line");
    }

    #[test]
    fn a_paper_plate_backs_the_label_with_3_px_to_spare_around_it() {
        let bars = bars_of(&analysis(vec![
            split(1, 1000.0, 300.0),
            split(2, 1000.0, 600.0),
        ]));
        // `AVG 7:30` is 8 Plex Mono glyphs of 0.6 em at 12 px; its figures
        // are 8.4 px tall.
        let width = 8.0 * 0.6 * 12.0;
        let (x, y, w, h) = bars.avg_label_plate;
        assert_near(x, 688.0 - width - 3.0, "plate x");
        assert_near(w, width + 6.0, "plate width");
        assert_near(y, bars.avg_label_y - 8.4 - 3.0, "plate y");
        assert_near(h, 8.4 + 6.0, "plate height");
        assert!(y + h < bars.avg_y - 1.0, "the plate covers the line");
    }

    #[test]
    fn the_plate_below_the_line_leaves_the_line_free() {
        let bars = bars_of(&fast_finish());
        let (_, y, _, _) = bars.avg_label_plate;
        assert!(y > bars.avg_y + 1.0, "plate at {y}, line at {}", bars.avg_y);
    }

    #[test]
    fn the_fastest_full_kilometre_has_a_dot_12_px_above_its_bar() {
        let analysis = analysis(vec![
            split(1, 1000.0, 330.0),
            split(2, 1000.0, 300.0),
            split(3, 1000.0, 600.0),
        ]);
        let bars = bars_of(&analysis);
        let bar = &bars.bars[1];
        let (x, y) = bars.fastest_dot.expect("a fastest dot");
        assert_near(x, bar.x + bar.width / 2.0, "dot x");
        assert_near(y, 178.0, "dot y");
    }

    #[test]
    fn a_run_under_a_kilometre_is_one_bar_half_way_without_a_dot() {
        // Without full kilometres every pace sits half way on the scale.
        let bars = bars_of(&analysis(vec![split(1, 600.0, 180.0)]));
        assert_eq!(heights(&bars), vec![225.0]);
        assert_eq!(bars.fastest_dot, None);
        assert_near(bars.avg_y, 335.0, "average line");
    }

    #[test]
    fn the_value_has_every_number_rounded_to_two_decimals() {
        let bars = Bars {
            bars: vec![Bar {
                index: 7,
                x: 32.123_4,
                y: 190.005_1,
                width: 13.302_3,
                height: 369.994_9,
                color: "#e8601c".into(),
            }],
            avg_y: 215.456_7,
            avg_label: "AVG 5:39".into(),
            avg_label_y: 209.456_7,
            avg_label_plate: (627.404_5, 198.056_7, 63.6, 14.4),
            fastest_dot: Some((38.774_5, 178.005_1)),
        };
        let value = bars.to_value();
        let attr = |value: &Value, name: &str| value.get_attr(name).unwrap();
        let bar = attr(&value, "bars").get_item(&Value::from(0)).unwrap();
        assert_eq!(attr(&bar, "index").to_string(), "7");
        assert_eq!(attr(&bar, "x").to_string(), "32.12");
        assert_eq!(attr(&bar, "y").to_string(), "190.01");
        assert_eq!(attr(&bar, "width").to_string(), "13.3");
        assert_eq!(attr(&bar, "height").to_string(), "369.99");
        assert_eq!(attr(&bar, "color").as_str(), Some("#e8601c"));
        assert_eq!(attr(&value, "avg_y").to_string(), "215.46");
        assert_eq!(attr(&value, "avg_label").as_str(), Some("AVG 5:39"));
        assert_eq!(attr(&value, "avg_label_y").to_string(), "209.46");
        let plate = attr(&value, "avg_label_plate");
        let plate: Vec<String> = (0..4)
            .map(|i| plate.get_item(&Value::from(i)).unwrap().to_string())
            .collect();
        assert_eq!(plate, ["627.4", "198.06", "63.6", "14.4"]);
        let dot = attr(&value, "fastest_dot");
        assert_eq!(dot.get_item(&Value::from(0)).unwrap().to_string(), "38.77");
        assert_eq!(dot.get_item(&Value::from(1)).unwrap().to_string(), "178.01");
    }

    #[test]
    fn without_a_fastest_kilometre_the_value_has_no_dot() {
        let bars = bars_of(&analysis(vec![split(1, 600.0, 180.0)]));
        assert!(bars.to_value().get_attr("fastest_dot").unwrap().is_none());
    }
}
