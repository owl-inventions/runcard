//! Lays out the `wave` variant: the pace as a smooth curve over the distance.

use minijinja::{context, Value};
use serde::Serialize;

use crate::model::{Analysis, Meta, Split, SplitView};
use crate::palette::{self, Palette};
use crate::render::px;
use crate::text::{Role, TextMeasure};

/// Left end of the distance axis, where the run starts.
const LEFT: f32 = 48.0;
/// Right end of the distance axis, where the run ends.
const RIGHT: f32 = 672.0;
/// Height of a pace at the slow anchor or slower.
const SLOW_Y: f32 = 684.0;
/// Height of a pace at the fast anchor or faster: fast is up.
const FAST_Y: f32 = 240.0;
/// Where the fill under the curve ends, 20 px above the bottom of the zone.
const FLOOR: f32 = 704.0;
/// Half the curve's 4 px stroke: how far its round caps reach past its ends.
const CAP: f32 = 2.0;
/// A Catmull-Rom tangent is half the chord between a point's neighbours; a
/// Bézier control point lies a third of the tangent away from its point.
const TANGENT: f32 = 0.5 / 3.0;
/// The label of the average line: Label at 11 px, its lowercase
/// letters 0.53 em high, the `g` reaching 0.21 em below the baseline.
const LABEL: &str = "average";
const LABEL_SIZE: f32 = 11.0;
const LABEL_ASCENT: f32 = 0.53 * LABEL_SIZE;
const LABEL_DESCENT: f32 = 0.21 * LABEL_SIZE;
/// Baseline of the label above the line, or below it when the curve is in the way.
const LABEL_ABOVE: f32 = 6.0;
const LABEL_BELOW: f32 = 12.0;
/// How far the label steps left along the line while the curve is in its way.
const LABEL_STEP: f32 = 8.0;
/// Room the label keeps from the curve's centre line: half the 4 px stroke
/// and 2 px of paper.
const CURVE_CLEARANCE: f32 = 2.0 + 2.0;

/// A position on the card.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// A cubic Bézier curve from `from` to `to` with the control points `c1` and `c2`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Cubic {
    pub from: Point,
    pub c1: Point,
    pub c2: Point,
    pub to: Point,
}

impl Cubic {
    /// The point at parameter `f`, 0 at `from` to 1 at `to`.
    fn at(&self, f: f32) -> Point {
        let g = 1.0 - f;
        let (a, b, c, d) = (g * g * g, 3.0 * g * g * f, 3.0 * g * f * f, f * f * f);
        Point {
            x: a * self.from.x + b * self.c1.x + c * self.c2.x + d * self.to.x,
            y: a * self.from.y + b * self.c1.y + c * self.c2.y + d * self.to.y,
        }
    }

    /// The length of the control polygon, which the curve does not exceed.
    fn hull_length(&self) -> f32 {
        let length = |a: Point, b: Point| (b.x - a.x).hypot(b.y - a.y);
        length(self.from, self.c1) + length(self.c1, self.c2) + length(self.c2, self.to)
    }
}

/// The part of the curve that leads into one kilometre, in its colour.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Section {
    pub curve: Cubic,
    /// CSS colour of the pace of the kilometre the section leads into.
    pub color: String,
}

/// The `average` label of the average line.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Label {
    /// Right end.
    pub x: f32,
    /// Baseline.
    pub y: f32,
    /// The curve runs through the label wherever it goes along the line, so
    /// it is set over the curve with a paper halo.
    pub halo: bool,
}

/// What `templates/wave.svg` draws inside the visualisation zone.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Wave {
    /// The curve through the kilometres' points, one section from each point
    /// to the next; for one split a single horizontal one across the zone,
    /// its round caps ending at the zone's edges.
    pub sections: Vec<Section>,
    /// Where the fill under the curve ends at the bottom.
    pub floor: f32,
    /// Height of the dashed line of the average pace.
    pub avg_y: f32,
    /// Left and right end of the average line.
    pub line_from: f32,
    pub line_to: f32,
    /// Where the `average` label goes.
    pub label: Label,
}

/// Lays out the wave in card pixels.
///
/// Each kilometre is a point at the middle of its stretch of the distance
/// (so the partial one sits in a narrow stretch at the end) and at the height
/// of its palette position; the curve is the Catmull-Rom spline through the
/// points, held within their heights (see [`spline`]). The average line sits
/// at the height of the average pace's palette position, its label just
/// above its right end unless the curve is there.
pub fn build(
    analysis: &Analysis,
    splits: &[SplitView],
    _palette: &Palette,
    measure: &TextMeasure,
    _meta: &Meta,
) -> Wave {
    let points = points(&analysis.splits, splits);
    let sections: Vec<Section> = match (&points[..], splits) {
        ([only], [view]) => vec![Section {
            curve: straight(
                Point {
                    x: LEFT + CAP,
                    y: only.y,
                },
                Point {
                    x: RIGHT - CAP,
                    y: only.y,
                },
            ),
            color: view.color.clone(),
        }],
        _ => spline(&points)
            .into_iter()
            .zip(&splits[1.min(splits.len())..])
            .map(|(curve, view)| Section {
                curve,
                color: view.color.clone(),
            })
            .collect(),
    };
    let avg_y = y_of(average_position(analysis));
    let width = measure.width(Role::Label, LABEL, LABEL_SIZE);
    Wave {
        label: label_position(&sections, avg_y, width),
        sections,
        floor: FLOOR,
        avg_y,
        line_from: LEFT,
        line_to: RIGHT,
    }
}

/// One point per split: x at the middle of its stretch of the whole
/// distance, y at the height of its palette position.
fn points(splits: &[Split], views: &[SplitView]) -> Vec<Point> {
    let total: f64 = splits.iter().map(|s| s.distance_m).sum();
    let mut covered = 0.0;
    splits
        .iter()
        .zip(views)
        .map(|(split, view)| {
            let middle = covered + split.distance_m / 2.0;
            covered += split.distance_m;
            Point {
                x: LEFT + (RIGHT - LEFT) * (middle / total) as f32,
                y: y_of(view.t),
            }
        })
        .collect()
}

/// The Catmull-Rom spline through `points` as cubic Béziers, one from each
/// point to the next; the end points stand in for their missing neighbours.
///
/// The tangents are limited in height so the curve never rises or falls
/// beyond the points it joins: it is flat at a peak or a valley, and no
/// steeper at a point than the rise to its nearer neighbour allows. A
/// kilometre is never drawn slower or faster than it was.
pub fn spline(points: &[Point]) -> Vec<Cubic> {
    let offsets: Vec<Point> = (0..points.len()).map(|i| offset(points, i)).collect();
    points
        .windows(2)
        .zip(offsets.windows(2))
        .map(|(pair, offset)| {
            let (from, to) = (pair[0], pair[1]);
            Cubic {
                from,
                c1: Point {
                    x: from.x + offset[0].x,
                    y: from.y + offset[0].y,
                },
                c2: Point {
                    x: to.x - offset[1].x,
                    y: to.y - offset[1].y,
                },
                to,
            }
        })
        .collect()
}

/// How far the Bézier control after point `i` lies from it (and the one
/// before it, mirrored): a sixth of the chord between its neighbours, its
/// height limited as [`spline`] describes.
fn offset(points: &[Point], i: usize) -> Point {
    let last = points.len() - 1;
    let (before, at, after) = (
        points[i.saturating_sub(1)],
        points[i],
        points[(i + 1).min(last)],
    );
    let x = (after.x - before.x) * TANGENT;
    let y = (after.y - before.y) * TANGENT;
    if i == 0 || i == last {
        // A sixth of the rise to the only neighbour stays within it.
        return Point { x, y };
    }
    let (rise_in, rise_out) = (at.y - before.y, after.y - at.y);
    let y = if rise_in * rise_out <= 0.0 {
        0.0
    } else {
        y.signum() * y.abs().min(rise_in.abs()).min(rise_out.abs())
    };
    Point { x, y }
}

/// The straight line from `from` to `to` as a cubic Bézier.
fn straight(from: Point, to: Point) -> Cubic {
    let at = |f: f32| Point {
        x: from.x + (to.x - from.x) * f,
        y: from.y + (to.y - from.y) * f,
    };
    Cubic {
        from,
        c1: at(1.0 / 3.0),
        c2: at(2.0 / 3.0),
        to,
    }
}

/// Height of palette position `t`, 0 at the slow anchor to 1 at the fast one.
fn y_of(t: f32) -> f32 {
    SLOW_Y + (FAST_Y - SLOW_Y) * t
}

/// Palette position of the average pace between the anchors of the full
/// kilometres: `CardData::avg_t`, which the layout is not handed.
fn average_position(analysis: &Analysis) -> f32 {
    let average = analysis.elapsed_s / analysis.distance_m * 1000.0;
    let full: Vec<f64> = analysis
        .splits
        .iter()
        .filter(|s| !s.partial)
        .map(Split::pace_s_per_km)
        .collect();
    let (slow, fast) = palette::anchors(&full);
    palette::pace_position(average, slow, fast)
}

/// The label of the average line at `avg_y`, `width` px wide: just above
/// the line's right end, else just below it, else stepping left along the
/// line until the curve is clear of it; above the right end on a halo if it
/// never is.
fn label_position(sections: &[Section], avg_y: f32, width: f32) -> Label {
    let mut right = RIGHT;
    while right - width >= LEFT {
        for baseline in [avg_y - LABEL_ABOVE, avg_y + LABEL_BELOW] {
            let at = Point {
                x: right,
                y: baseline,
            };
            if is_clear(sections, at, width) {
                return Label {
                    x: at.x,
                    y: at.y,
                    halo: false,
                };
            }
        }
        right -= LABEL_STEP;
    }
    Label {
        x: RIGHT,
        y: avg_y - LABEL_ABOVE,
        halo: true,
    }
}

/// Whether the label ending at `label.x` on the baseline `label.y` keeps
/// its room from the curve.
fn is_clear(sections: &[Section], label: Point, width: f32) -> bool {
    let (left, top, bottom) = (
        label.x - width,
        label.y - LABEL_ASCENT,
        label.y + LABEL_DESCENT,
    );
    // Distance from `p` to the label's box, 0 inside it.
    let distance = |p: Point| {
        let dx = (left - p.x).max(p.x - label.x).max(0.0);
        let dy = (top - p.y).max(p.y - bottom).max(0.0);
        dx.hypot(dy)
    };
    sections.iter().all(|section| {
        let curve = &section.curve;
        // Samples about a pixel apart along the curve.
        let steps = curve.hull_length().ceil().max(1.0) as u32;
        (0..=steps).all(|i| distance(curve.at(i as f32 / steps as f32)) > CURVE_CLEARANCE)
    })
}

/// The SVG path data of `curve` after the pen is at its start: `C` and its
/// three points.
fn curve_to(curve: &Cubic) -> String {
    format!(
        "C{} {} {}",
        coords(curve.c1),
        coords(curve.c2),
        coords(curve.to)
    )
}

/// `x y`, each rounded to two decimals.
fn coords(p: Point) -> String {
    format!("{} {}", px(p.x), px(p.y))
}

impl Wave {
    /// The block as the template sees it, under the key `wave`: each section
    /// as path data `d` and its colour, the fill under the whole curve as
    /// path data, every number rounded to two decimals.
    pub fn to_value(&self) -> Value {
        let sections: Vec<Value> = self
            .sections
            .iter()
            .map(|section| {
                context! {
                    d => format!("M{}{}", coords(section.curve.from), curve_to(&section.curve)),
                    color => &section.color,
                }
            })
            .collect();
        let fill = match (self.sections.first(), self.sections.last()) {
            (Some(first), Some(last)) => {
                let curve: String = self.sections.iter().map(|s| curve_to(&s.curve)).collect();
                format!(
                    "M{}{curve}L{} {}L{} {}Z",
                    coords(first.curve.from),
                    px(last.curve.to.x),
                    px(self.floor),
                    px(first.curve.from.x),
                    px(self.floor),
                )
            }
            _ => String::new(),
        };
        context! {
            sections => sections,
            fill => fill,
            avg_y => px(self.avg_y),
            line_from => px(self.line_from),
            line_to => px(self.line_to),
            label => context! {
                x => px(self.label.x),
                y => px(self.label.y),
                halo => self.label.halo,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card;
    use crate::model::{CardData, Split};
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

    /// Five kilometres at 5:00, 5:40, 4:50, 5:20 and 5:10, then 500 m at 5:30.
    fn five_and_a_half() -> Analysis {
        analysis(vec![
            split(1, 1000.0, 300.0),
            split(2, 1000.0, 340.0),
            split(3, 1000.0, 290.0),
            split(4, 1000.0, 320.0),
            split(5, 1000.0, 310.0),
            split(6, 500.0, 165.0),
        ])
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
            Variant::Wave,
            Theme::Light,
        )
    }

    fn wave_of(analysis: &Analysis) -> Wave {
        card_of(analysis).wave.expect("the wave block")
    }

    fn point(x: f32, y: f32) -> Point {
        Point { x, y }
    }

    fn assert_near(actual: f32, expected: f32, what: &str) {
        assert!(
            (actual - expected).abs() < 1e-3,
            "{what} was {actual}, expected {expected}"
        );
    }

    fn assert_point(actual: Point, expected: Point, what: &str) {
        assert_near(actual.x, expected.x, &format!("x of {what}"));
        assert_near(actual.y, expected.y, &format!("y of {what}"));
    }

    /// The points the curve runs through: where each section starts, and
    /// where the last one ends.
    fn points(wave: &Wave) -> Vec<Point> {
        let mut points: Vec<Point> = wave.sections.iter().map(|s| s.curve.from).collect();
        points.extend(wave.sections.last().map(|s| s.curve.to));
        points
    }

    #[test]
    fn the_slow_anchor_is_at_y_684_and_the_fast_one_at_y_240() {
        assert_near(y_of(0.0), 684.0, "slow anchor");
        assert_near(y_of(0.5), 462.0, "half way");
        assert_near(y_of(1.0), 240.0, "fast anchor");
    }

    #[test]
    fn each_point_sits_at_the_middle_of_its_kilometre_over_the_whole_distance() {
        let card = card_of(&five_and_a_half());
        let wave = card.wave.expect("the wave block");
        let points = points(&wave);
        assert_eq!(points.len(), 6);
        // 5.5 km over 624 px: a full kilometre is 113.45 px, the partial half that.
        let km = 624.0 / 5.5;
        for (i, (point, view)) in points.iter().zip(&card.splits).enumerate() {
            let what = format!("km {}", view.index);
            let middle = if view.partial { 5.25 } else { i as f32 + 0.5 };
            assert_near(point.x, 48.0 + middle * km, &what);
            assert_near(point.y, 684.0 - 444.0 * view.t, &what);
        }
    }

    #[test]
    fn a_section_leads_from_each_point_to_the_next_in_the_colour_of_the_next_kilometre() {
        let card = card_of(&five_and_a_half());
        let wave = card.wave.expect("the wave block");
        assert_eq!(wave.sections.len(), 5);
        for (i, section) in wave.sections.iter().enumerate() {
            assert_eq!(section.color, card.splits[i + 1].color, "section {i}");
        }
    }

    #[test]
    fn the_spline_passes_through_its_points_at_the_ends_of_its_segments() {
        let points = [
            point(60.0, 400.0),
            point(150.0, 250.0),
            point(260.0, 600.0),
            point(330.0, 300.0),
            point(400.0, 320.0),
        ];
        let curves = spline(&points);
        assert_eq!(curves.len(), 4);
        for (i, curve) in curves.iter().enumerate() {
            assert_point(curve.from, points[i], &format!("start of segment {i}"));
            assert_point(curve.to, points[i + 1], &format!("end of segment {i}"));
        }
    }

    #[test]
    fn the_spline_has_catmull_rom_tangents_and_no_kink_at_its_points() {
        // Falling all the way, never steeper than the neighbouring rises allow.
        let points = [
            point(60.0, 600.0),
            point(150.0, 500.0),
            point(260.0, 350.0),
            point(330.0, 300.0),
        ];
        let curves = spline(&points);
        // Inside: a sixth of the chord between the neighbours.
        assert_point(
            curves[1].c1,
            point(150.0 + 200.0 / 6.0, 500.0 - 250.0 / 6.0),
            "first control of segment 1",
        );
        assert_point(
            curves[1].c2,
            point(260.0 - 180.0 / 6.0, 350.0 + 200.0 / 6.0),
            "second control of segment 1",
        );
        // At the ends the point is its own missing neighbour.
        assert_point(
            curves[0].c1,
            point(60.0 + 90.0 / 6.0, 600.0 - 100.0 / 6.0),
            "first control of the curve",
        );
        assert_point(
            curves[2].c2,
            point(330.0 - 70.0 / 6.0, 300.0 + 50.0 / 6.0),
            "last control of the curve",
        );
        assert_no_kinks(&curves);
    }

    /// Where two segments meet, their controls mirror each other.
    fn assert_no_kinks(curves: &[Cubic]) {
        for (i, pair) in curves.windows(2).enumerate() {
            let joint = pair[0].to;
            let (before, after) = (pair[0].c2, pair[1].c1);
            assert_point(
                point(joint.x - before.x, joint.y - before.y),
                point(after.x - joint.x, after.y - joint.y),
                &format!("tangents at point {}", i + 1),
            );
        }
    }

    #[test]
    fn the_spline_is_flat_at_a_peak_or_a_valley() {
        let points = [
            point(60.0, 400.0),
            point(150.0, 250.0),
            point(260.0, 600.0),
            point(330.0, 300.0),
        ];
        let curves = spline(&points);
        assert_near(curves[0].c2.y, 250.0, "into the peak at point 1");
        assert_near(curves[1].c1.y, 250.0, "out of the peak at point 1");
        assert_near(curves[1].c2.y, 600.0, "into the valley at point 2");
        assert_near(curves[2].c1.y, 600.0, "out of the valley at point 2");
        assert_no_kinks(&curves);
    }

    #[test]
    fn a_tangent_is_no_steeper_than_the_smaller_rise_to_a_neighbour() {
        // A sixth of the chord would be 65 px, the rise into point 1 is 10.
        let points = [point(60.0, 500.0), point(150.0, 490.0), point(260.0, 100.0)];
        let curves = spline(&points);
        assert_near(curves[0].c2.y, 500.0, "into point 1");
        assert_near(curves[1].c1.y, 480.0, "out of point 1");
        assert_no_kinks(&curves);
    }

    #[test]
    fn the_spline_never_leaves_the_heights_of_the_points_it_joins() {
        // Steep falls into and out of a slow plateau, as on a climb.
        let points = [
            point(60.0, 400.0),
            point(100.0, 230.0),
            point(140.0, 680.0),
            point(180.0, 680.0),
            point(220.0, 670.0),
            point(260.0, 230.0),
            point(300.0, 240.0),
        ];
        for (i, curve) in spline(&points).iter().enumerate() {
            let (low, high) = (curve.from.y.min(curve.to.y), curve.from.y.max(curve.to.y));
            for step in 0..=100 {
                let y = curve.at(step as f32 / 100.0).y;
                assert!(
                    (low - 1e-3..=high + 1e-3).contains(&y),
                    "segment {i} reaches y {y} outside {low}..{high}"
                );
            }
        }
    }

    #[test]
    fn a_spline_needs_two_points() {
        assert!(spline(&[]).is_empty());
        assert!(spline(&[point(60.0, 400.0)]).is_empty());
    }

    #[test]
    fn a_single_split_is_a_horizontal_line_across_the_zone_at_its_height_caps_included() {
        let card = card_of(&analysis(vec![split(1, 600.0, 180.0)]));
        let wave = card.wave.expect("the wave block");
        assert_eq!(wave.sections.len(), 1);
        let curve = wave.sections[0].curve;
        let y = 684.0 - 444.0 * card.splits[0].t;
        for (what, p) in [
            ("from", curve.from),
            ("c1", curve.c1),
            ("c2", curve.c2),
            ("to", curve.to),
        ] {
            assert_near(p.y, y, what);
        }
        // Inset by half the stroke, so its round caps end at x 48 and 672.
        assert_near(curve.from.x, 50.0, "left end");
        assert_near(curve.to.x, 670.0, "right end");
        assert_eq!(wave.sections[0].color, card.splits[0].color);
    }

    #[test]
    fn the_average_line_spans_the_zone_at_the_average_palette_position() {
        let card = card_of(&five_and_a_half());
        let wave = card.wave.expect("the wave block");
        assert_near(wave.avg_y, 684.0 - 444.0 * card.avg_t, "average line");
        assert_near(wave.line_from, 48.0, "left end");
        assert_near(wave.line_to, 672.0, "right end");
        // 20 px under the slow anchor and 20 px above the bottom of the zone.
        assert_near(wave.floor, 704.0, "floor of the fill");
    }

    #[test]
    fn the_curve_and_its_fill_stay_inside_the_zone() {
        // Forty-two kilometres, the first the fastest: its point near x 48.
        let first = analysis(
            (1..=42)
                .map(|km| {
                    split(
                        km,
                        1000.0,
                        if km == 1 {
                            270.0
                        } else {
                            300.0 + f64::from(km % 7) * 5.0
                        },
                    )
                })
                .collect(),
        );
        for run in [five_and_a_half(), first] {
            let wave = wave_of(&run);
            for p in points(&wave) {
                // Between the anchors, the 4 px stroke reaching 2 px past them.
                assert!((50.0..=670.0).contains(&p.x), "point at x {}", p.x);
                assert!((240.0..=684.0).contains(&p.y), "point at y {}", p.y);
            }
            assert!(wave.floor <= 724.0 && (200.0..=724.0).contains(&wave.avg_y));
        }
    }

    /// A straight section from (`x0`, `y0`) to (`x1`, `y1`).
    fn line(x0: f32, y0: f32, x1: f32, y1: f32) -> Section {
        let at = |f: f32| point(x0 + (x1 - x0) * f, y0 + (y1 - y0) * f);
        Section {
            curve: Cubic {
                from: at(0.0),
                c1: at(1.0 / 3.0),
                c2: at(2.0 / 3.0),
                to: at(1.0),
            },
            color: "#5fa39f".into(),
        }
    }

    /// A label ending at (`x`, `y`), over a halo if `halo`.
    fn label(x: f32, y: f32, halo: bool) -> Label {
        Label { x, y, halo }
    }

    #[test]
    fn the_label_sits_just_above_the_right_end_of_the_line_when_the_curve_is_clear() {
        let far = [line(60.0, 250.0, 672.0, 260.0)];
        assert_eq!(
            label_position(&far, 400.0, 40.0),
            label(672.0, 394.0, false)
        );
    }

    #[test]
    fn the_label_goes_under_the_line_when_the_curve_runs_just_above_it() {
        let above = [line(60.0, 391.0, 672.0, 391.0)];
        assert_eq!(
            label_position(&above, 400.0, 40.0),
            label(672.0, 412.0, false)
        );
    }

    #[test]
    fn the_label_moves_left_when_the_curve_crosses_the_line_at_its_right_end() {
        let crossing = [line(600.0, 300.0, 672.0, 500.0)];
        let label = label_position(&crossing, 400.0, 40.0);
        // The curve crosses the line at x 636.
        assert!(
            (560.0..640.0).contains(&label.x),
            "the label ends at x {}",
            label.x
        );
        assert!(label.y == 394.0 || label.y == 412.0, "baseline {}", label.y);
        assert!(!label.halo);
    }

    #[test]
    fn a_label_the_curve_runs_through_wherever_it_goes_sits_above_the_right_end_on_a_halo() {
        // Steep strokes every 15 px across the whole line, as on a marathon
        // of alternating paces.
        let zigzag: Vec<Section> = (0..42)
            .map(|i| {
                let x = 48.0 + 15.0 * i as f32;
                let (from, to) = if i % 2 == 0 {
                    (200.0, 680.0)
                } else {
                    (680.0, 200.0)
                };
                line(x, from, x + 15.0, to)
            })
            .collect();
        assert_eq!(
            label_position(&zigzag, 400.0, 40.0),
            label(672.0, 394.0, true)
        );
    }

    #[test]
    fn the_template_value_has_the_path_data_rounded_to_two_decimals() {
        let wave = Wave {
            sections: vec![
                line(48.0, 300.0, 100.0, 400.0),
                line(100.0, 400.0, 150.123, 350.456),
            ],
            floor: 700.0,
            avg_y: 433.333,
            line_from: 48.0,
            line_to: 672.0,
            label: label(672.0, 427.333, true),
        };
        let value = wave.to_value();
        let section = value
            .get_attr("sections")
            .unwrap()
            .get_item(&Value::from(1))
            .unwrap();
        assert_eq!(
            section.get_attr("d").unwrap().as_str(),
            Some("M100 400C116.71 383.49 133.42 366.97 150.12 350.46")
        );
        assert_eq!(section.get_attr("color").unwrap().as_str(), Some("#5fa39f"));
        assert_eq!(
            value.get_attr("fill").unwrap().as_str(),
            Some(
                "M48 300C65.33 333.33 82.67 366.67 100 400\
                 C116.71 383.49 133.42 366.97 150.12 350.46\
                 L150.12 700L48 700Z"
            )
        );
        assert_eq!(value.get_attr("avg_y").unwrap().to_string(), "433.33");
        let label = value.get_attr("label").unwrap();
        assert_eq!(label.get_attr("y").unwrap().to_string(), "427.33");
        assert!(label.get_attr("halo").unwrap().is_true());
    }
}
