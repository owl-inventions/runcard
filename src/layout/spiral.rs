//! Lays out the `spiral` variant: the kilometres as segments of one Archimedean spiral.
//!
//! The spiral `r = 24 + growth * angle` starts at 3 o'clock and winds
//! clockwise outward around the middle of the visualisation zone until it
//! reaches a radius of 254 px. Every kilometre takes a share of its arc
//! length proportional to its distance, so a longer run winds more tightly.

use std::f64::consts::{PI, TAU};
use std::iter;

use minijinja::{context, Value};
use serde::Serialize;

use crate::model::{Analysis, Meta, SplitView};
use crate::palette::Palette;
use crate::render::px;
use crate::route;
use crate::text::{Role, TextMeasure};

/// Centre of the spiral, the middle of the visualisation zone x 48 to 672,
/// y 200 to 724.
const CENTRE: (f64, f64) = (360.0, 462.0);
/// Radius at which the spiral starts, at angle 0.
const START_RADIUS: f64 = 24.0;
/// Radius at which the spiral ends: the widest stroke then ends 262 px from
/// the centre, on the top or bottom edge of the zone.
const END_RADIUS: f64 = 262.0 - MAX_STROKE / 2.0;
/// The steepest spiral: px of radius gained per radian.
const MAX_GROWTH: f64 = 12.0;
/// Arc length in px a full kilometre asks for. The spiral is the steepest
/// one, up to [`MAX_GROWTH`], long enough for every kilometre to get it; it
/// then runs on to [`END_RADIUS`], so the kilometres share whatever arc that
/// spiral has.
const KM_LENGTH: f64 = 400.0;
/// The spiral is sampled every half degree.
const STEP: f64 = PI / 360.0;
/// Paper cut off a segment at an end it shares with a neighbour: one degree,
/// so neighbours are two degrees apart.
const HALF_GAP: f64 = PI / 180.0;
/// Share of the distance between neighbouring turns the stroke covers.
const STROKE_SHARE: f64 = 0.75;
const MIN_STROKE: f64 = 4.0;
const MAX_STROKE: f64 = 16.0;
/// The flattest spiral: its stroke at [`MIN_STROKE`] still leaves a quarter of
/// the distance between turns as paper, however long the run.
const MIN_GROWTH: f64 = MIN_STROKE / (STROKE_SHARE * TAU);
/// Bisection steps when searching the growth.
const BISECTION_STEPS: usize = 40;
/// Paper between the stroke of the fastest kilometre and its ring.
const RING_CLEARANCE: f32 = 4.0;
/// The pace beside the ring: Mono at 12 px, placed 10 px beyond the ring,
/// its baseline 4.5 px below that point so the figures centre on it.
const LABEL_SIZE: f32 = 12.0;
const LABEL_GAP: f32 = 10.0;
const LABEL_DROP: f32 = 4.5;
/// Height of the figures of JetBrains Mono at 12 px (cap height 0.73 em).
const FIGURE_HEIGHT: f32 = 8.76;
/// The zone the picture, the pace included, stays in.
const ZONE_LEFT: f32 = 48.0;
const ZONE_RIGHT: f32 = 672.0;
const ZONE_TOP: f32 = 200.0;
const ZONE_BOTTOM: f32 = 724.0;

/// One kilometre of the spiral.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Segment {
    /// 1-based kilometre number.
    pub index: u32,
    /// SVG path data of the kilometre's polyline.
    pub path: String,
    /// CSS colour of the kilometre's pace.
    pub color: String,
}

/// A circle drawn as an outline.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Ring {
    pub x: f32,
    pub y: f32,
    pub r: f32,
}

/// A text placed by its anchor on the baseline.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Label {
    pub x: f32,
    /// The baseline.
    pub y: f32,
    /// The SVG `text-anchor`, `start` or `end`.
    pub anchor: &'static str,
    pub text: String,
}

/// The fastest full kilometre: a ring around its middle and its pace beside it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Callout {
    pub ring: Ring,
    /// The pace, e.g. `4:48`.
    pub label: Label,
}

/// What `templates/spiral.svg` draws inside the visualisation zone.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Spiral {
    /// One polyline per split, in order, a little paper between neighbours.
    pub segments: Vec<Segment>,
    /// Stroke width of every segment in px.
    pub stroke_width: f32,
    /// Centre of the start dot, the first point of the spiral.
    pub start: Option<(f32, f32)>,
    /// Centre of the finish square, the last point of the spiral.
    pub finish: Option<(f32, f32)>,
    /// The ring around the middle of the fastest full kilometre and its pace.
    pub fastest: Option<Callout>,
}

/// Lays out the spiral in card pixels: one segment per split in its colour,
/// the start and finish markers and the callout of the fastest kilometre.
pub fn build(
    analysis: &Analysis,
    splits: &[SplitView],
    _palette: &Palette,
    measure: &TextMeasure,
    _meta: &Meta,
) -> Spiral {
    let fractions: Vec<f64> = splits.iter().map(|s| f64::from(s.fraction)).collect();
    let Some(coil) = coil(&fractions) else {
        return Spiral {
            segments: Vec::new(),
            stroke_width: 0.0,
            start: None,
            finish: None,
            fastest: None,
        };
    };
    let card = |(x, y): (f64, f64)| (x as f32, y as f32);
    let segments = coil
        .lines
        .iter()
        .zip(splits)
        .map(|(line, view)| {
            let points: Vec<(f32, f32)> = line.iter().copied().map(card).collect();
            Segment {
                index: view.index,
                path: route::to_path(&points),
                color: view.color.clone(),
            }
        })
        .collect();
    let fastest = analysis
        .fastest
        .and_then(|index| splits.iter().position(|s| s.index == index))
        .map(|i| {
            let (x, y) = card(coil.middles[i]);
            let ring = Ring {
                x,
                y,
                r: coil.width / 2.0 + RING_CLEARANCE,
            };
            let pace = &splits[i].pace;
            let width = measure.width(Role::Mono, pace, LABEL_SIZE);
            Callout {
                ring,
                label: label(ring, pace.clone(), width),
            }
        });
    Spiral {
        segments,
        stroke_width: coil.width,
        start: coil
            .lines
            .first()
            .and_then(|line| line.first())
            .copied()
            .map(card),
        finish: coil
            .lines
            .last()
            .and_then(|line| line.last())
            .copied()
            .map(card),
        fastest,
    }
}

/// The pace `text`, `width` px wide, beside the fastest kilometre's `ring`,
/// [`LABEL_GAP`] px beyond the ring on the radius through its centre:
/// outward, or inward where outward its figures would leave the zone at the
/// top or bottom. It reads away from the centre line, anchored at its start
/// from x 360 rightward, at its end left of it. A pace that would cross a
/// side of the zone is moved along its baseline until it ends on that side.
fn label(ring: Ring, text: String, width: f32) -> Label {
    let (cx, cy) = (CENTRE.0 as f32, CENTRE.1 as f32);
    let (dx, dy) = (ring.x - cx, ring.y - cy);
    let reach = (ring.r + LABEL_GAP) / dx.hypot(dy);
    let at = |side: f32| {
        (
            ring.x + side * dx * reach,
            ring.y + side * dy * reach + LABEL_DROP,
        )
    };
    let (x, y) = Some(at(1.0))
        .filter(|&(_, y)| y - FIGURE_HEIGHT >= ZONE_TOP && y <= ZONE_BOTTOM)
        .unwrap_or_else(|| at(-1.0));
    let anchor = if ring.x >= cx { "start" } else { "end" };
    let left = if anchor == "end" { x - width } else { x };
    let x = x + left.clamp(ZONE_LEFT, ZONE_RIGHT - width) - left;
    Label { x, y, anchor, text }
}

/// The spiral of a run, before it is turned into path data.
#[derive(Debug, Clone, PartialEq)]
struct Coil {
    /// Radius gained per radian, in px.
    growth: f64,
    /// Stroke width in px.
    width: f32,
    /// Arc length from the start to where each split begins, and to the end.
    arcs: Vec<f64>,
    /// The polyline of each split, the gaps between them cut away.
    lines: Vec<Vec<(f64, f64)>>,
    /// The point halfway along each split's share of the arc.
    middles: Vec<(f64, f64)>,
}

/// The spiral for splits of these `fractions` of a kilometre; none without
/// any distance to draw.
fn coil(fractions: &[f64]) -> Option<Coil> {
    let units: f64 = fractions.iter().sum();
    if fractions.is_empty() || units <= 0.0 {
        return None;
    }
    let growth = growth_for(units);
    let table = Table::of(growth);
    let total = table.length();
    let mut arcs: Vec<f64> = iter::once(0.0)
        .chain(fractions.iter().scan(0.0, |run, fraction| {
            *run += fraction;
            Some(*run / units * total)
        }))
        .collect();
    // The last split ends where the spiral does, whatever the rounding.
    arcs[fractions.len()] = total;
    let angles: Vec<f64> = arcs.iter().map(|&s| table.angle_at(s)).collect();
    let last = fractions.len() - 1;
    let lines = (0..fractions.len())
        .map(|i| {
            let (from, to) = (angles[i], angles[i + 1]);
            // A sliver of a split keeps half of itself.
            let cut = HALF_GAP.min((to - from) / 4.0);
            let from = if i > 0 { from + cut } else { from };
            let to = if i < last { to - cut } else { to };
            sample(growth, from, to)
        })
        .collect();
    let middles = arcs
        .windows(2)
        .map(|pair| point(growth, table.angle_at((pair[0] + pair[1]) / 2.0)))
        .collect();
    Some(Coil {
        growth,
        width: stroke_width(growth),
        arcs,
        lines,
        middles,
    })
}

/// The point of the spiral at `angle` radians: clockwise on screen, y down.
fn point(growth: f64, angle: f64) -> (f64, f64) {
    let r = START_RADIUS + growth * angle;
    (CENTRE.0 + r * angle.cos(), CENTRE.1 + r * angle.sin())
}

/// The angle at which the spiral reaches [`END_RADIUS`].
fn end_angle(growth: f64) -> f64 {
    (END_RADIUS - START_RADIUS) / growth
}

/// The angles the spiral is sampled at: every [`STEP`] from 0, and the end.
fn angles(growth: f64) -> impl Iterator<Item = f64> {
    let end = end_angle(growth);
    let steps = (end / STEP).ceil() as usize;
    (0..steps).map(|j| j as f64 * STEP).chain(iter::once(end))
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    (b.0 - a.0).hypot(b.1 - a.1)
}

/// Length of the spiral from the start to [`END_RADIUS`]: the sum of the
/// chords between its samples.
fn length(growth: f64) -> f64 {
    let mut previous = point(growth, 0.0);
    angles(growth)
        .skip(1)
        .map(|angle| {
            let next = point(growth, angle);
            let chord = distance(previous, next);
            previous = next;
            chord
        })
        .sum()
}

/// The largest growth up to [`MAX_GROWTH`] whose spiral gives `units`
/// kilometres [`KM_LENGTH`] each, found by bisection; the length falls as
/// the growth rises. Never below [`MIN_GROWTH`].
fn growth_for(units: f64) -> f64 {
    let wanted = KM_LENGTH * units;
    if length(MAX_GROWTH) >= wanted {
        return MAX_GROWTH;
    }
    if length(MIN_GROWTH) <= wanted {
        return MIN_GROWTH;
    }
    // `long` is long enough, `short` is not.
    let (mut long, mut short) = (MIN_GROWTH, MAX_GROWTH);
    for _ in 0..BISECTION_STEPS {
        let middle = (long + short) / 2.0;
        if length(middle) >= wanted {
            long = middle;
        } else {
            short = middle;
        }
    }
    long
}

/// The stroke width of a spiral: three quarters of the distance between its
/// turns, 4 to 16 px, to one decimal.
fn stroke_width(growth: f64) -> f32 {
    let width = (STROKE_SHARE * growth * TAU).clamp(MIN_STROKE, MAX_STROKE);
    ((width * 10.0).round() / 10.0) as f32
}

/// The points of the spiral from angle `from` to `to`: both ends and every
/// sample angle between them.
fn sample(growth: f64, from: f64, to: f64) -> Vec<(f64, f64)> {
    let first = (from / STEP).floor() as usize + 1;
    let inner = (first..)
        .map(|j| j as f64 * STEP)
        .take_while(|&angle| angle < to);
    iter::once(from)
        .chain(inner)
        .chain(iter::once(to))
        .map(|angle| point(growth, angle))
        .collect()
}

/// The sample angles of a spiral and the arc length up to each.
struct Table {
    angles: Vec<f64>,
    arcs: Vec<f64>,
}

impl Table {
    fn of(growth: f64) -> Self {
        let angles: Vec<f64> = angles(growth).collect();
        let arcs = iter::once(0.0)
            .chain(angles.windows(2).scan(0.0, |run, pair| {
                *run += distance(point(growth, pair[0]), point(growth, pair[1]));
                Some(*run)
            }))
            .collect();
        Self { angles, arcs }
    }

    /// The length of the whole spiral.
    fn length(&self) -> f64 {
        self.arcs[self.arcs.len() - 1]
    }

    /// The angle at arc length `s`, interpolated between the samples.
    fn angle_at(&self, s: f64) -> f64 {
        let last = self.arcs.len() - 1;
        let i = self.arcs.partition_point(|&arc| arc <= s).clamp(1, last);
        let (a0, a1) = (self.arcs[i - 1], self.arcs[i]);
        let along = if a1 > a0 {
            ((s - a0) / (a1 - a0)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.angles[i - 1] + along * (self.angles[i] - self.angles[i - 1])
    }
}

impl Spiral {
    /// The block as the template sees it, under the key `spiral`: the
    /// finish as the top-left corner of its 6 px square, every number
    /// rounded to two decimals.
    pub fn to_value(&self) -> Value {
        let segments: Vec<Value> = self
            .segments
            .iter()
            .map(|s| context! { index => s.index, path => &s.path, color => &s.color })
            .collect();
        context! {
            segments => segments,
            stroke_width => px(self.stroke_width),
            start => self.start.map(|(x, y)| context! { x => px(x), y => px(y) }),
            finish => self
                .finish
                .map(|(x, y)| context! { x => px(x - FINISH_HALF), y => px(y - FINISH_HALF) }),
            fastest => self.fastest.as_ref().map(|callout| {
                let (ring, label) = (callout.ring, &callout.label);
                context! {
                    x => px(ring.x),
                    y => px(ring.y),
                    r => px(ring.r),
                    label => context! {
                        x => px(label.x),
                        y => px(label.y),
                        anchor => label.anchor,
                        text => &label.text,
                    },
                }
            }),
        }
    }
}

/// Half the side of the finish square.
const FINISH_HALF: f32 = 3.0;

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    use crate::model::Split;
    use crate::text::Role;
    use crate::theme::Theme;

    const CENTRE: (f64, f64) = (360.0, 462.0);
    const END: f64 = 254.0;

    /// `full` whole kilometres and, if `partial` is above 0, a trailing partial
    /// of that fraction.
    fn fractions(full: usize, partial: f64) -> Vec<f64> {
        let mut fractions = vec![1.0; full];
        if partial > 0.0 {
            fractions.push(partial);
        }
        fractions
    }

    fn radius((x, y): (f64, f64)) -> f64 {
        (x - CENTRE.0).hypot(y - CENTRE.1)
    }

    /// Angle of `p` around the centre, clockwise on screen from 3 o'clock.
    fn angle((x, y): (f64, f64)) -> f64 {
        (y - CENTRE.1).atan2(x - CENTRE.0)
    }

    fn polyline_length(points: &[(f64, f64)]) -> f64 {
        points
            .windows(2)
            .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
            .sum()
    }

    /// Arc length of r = 24 + growth * angle from angle 0 to `angle`, in closed form.
    fn exact_length(growth: f64, angle: f64) -> f64 {
        let primitive = |angle: f64| {
            let r = 24.0 + growth * angle;
            let root = r.hypot(growth);
            (r * root + growth * growth * (r + root).ln()) / (2.0 * growth)
        };
        primitive(angle) - primitive(0.0)
    }

    fn coil_of(fractions: &[f64]) -> Coil {
        coil(fractions).expect("a coil for some splits")
    }

    /// The runs the tests draw: 5, 10, a half, a marathon and 110 km.
    fn runs() -> Vec<Vec<f64>> {
        vec![
            fractions(5, 0.0),
            fractions(10, 0.0),
            fractions(21, 0.1),
            fractions(42, 0.195),
            fractions(110, 0.0),
        ]
    }

    #[test]
    fn the_spiral_starts_at_radius_24_at_3_oclock_and_ends_on_radius_254() {
        for fractions in runs() {
            let coil = coil_of(&fractions);
            let first = coil.lines[0][0];
            assert!((first.0 - 384.0).abs() < 1e-9, "{first:?}");
            assert!((first.1 - 462.0).abs() < 1e-9, "{first:?}");
            let last = *coil.lines.last().unwrap().last().unwrap();
            let r = radius(last);
            assert!(
                (r - END).abs() <= 0.5,
                "{} km end at r {r}",
                fractions.len()
            );
        }
    }

    #[test]
    fn the_spiral_turns_clockwise_outward() {
        let coil = coil_of(&fractions(21, 0.1));
        let line = &coil.lines[0];
        // A quarter turn on: below the centre on screen, further out.
        let quarter = line
            .iter()
            .find(|&&p| angle(p) > 1.5)
            .expect("km 1 reaches a quarter turn");
        assert!(quarter.1 > CENTRE.1 + 20.0, "{quarter:?}");
        assert!(radius(*quarter) > 24.0, "{quarter:?}");
        let radii: Vec<f64> = coil.lines.iter().flatten().map(|&p| radius(p)).collect();
        assert!(radii.windows(2).all(|w| w[1] >= w[0] - 1e-9), "not outward");
    }

    #[test]
    fn there_is_one_segment_per_split() {
        for fractions in runs() {
            let coil = coil_of(&fractions);
            assert_eq!(coil.lines.len(), fractions.len());
            assert_eq!(coil.middles.len(), fractions.len());
            assert_eq!(coil.arcs.len(), fractions.len() + 1);
            assert!(coil.lines.iter().all(|line| line.len() >= 2));
        }
    }

    #[test]
    fn each_split_takes_a_share_of_the_arc_proportional_to_its_fraction() {
        for fractions in runs() {
            let coil = coil_of(&fractions);
            let units: f64 = fractions.iter().sum();
            let total = coil.arcs[fractions.len()];
            let end = (END - 24.0) / coil.growth;
            let exact = exact_length(coil.growth, end);
            assert!((total - exact).abs() / exact < 1e-4, "{total} vs {exact}");
            assert_eq!(coil.arcs[0], 0.0);
            for (i, fraction) in fractions.iter().enumerate() {
                let share = coil.arcs[i + 1] - coil.arcs[i];
                let expected = total * fraction / units;
                assert!(
                    (share - expected).abs() < 1e-6 * total,
                    "km {}: {share} vs {expected}",
                    i + 1
                );
                // The drawn line is the share less up to 1 degree at either end.
                let drawn = polyline_length(&coil.lines[i]);
                let gaps = 2.0 * 1f64.to_radians() * (END + coil.growth);
                assert!(drawn <= share + 1e-6, "km {}: {drawn} > {share}", i + 1);
                assert!(drawn >= share - gaps, "km {}: {drawn} < {share}", i + 1);
            }
        }
    }

    #[test]
    fn neighbouring_segments_are_two_degrees_of_paper_apart() {
        for fractions in runs() {
            let coil = coil_of(&fractions);
            for (i, pair) in coil.lines.windows(2).enumerate() {
                let (end, start) = (*pair[0].last().unwrap(), pair[1][0]);
                let gap = (angle(start) - angle(end)).rem_euclid(TAU).to_degrees();
                assert!(
                    (gap - 2.0).abs() < 1e-6,
                    "after km {}: {gap} degrees",
                    i + 1
                );
            }
        }
    }

    #[test]
    fn the_lines_are_sampled_every_half_degree() {
        let coil = coil_of(&fractions(21, 0.1));
        for line in &coil.lines {
            for pair in line.windows(2) {
                let step = (angle(pair[1]) - angle(pair[0]))
                    .rem_euclid(TAU)
                    .to_degrees();
                assert!(step > 0.0 && step <= 0.5 + 1e-9, "{step} degrees");
            }
        }
    }

    #[test]
    fn a_longer_run_winds_more_tightly() {
        let growths: Vec<f64> = runs().iter().map(|f| coil_of(f).growth).collect();
        assert!(growths.windows(2).all(|w| w[1] < w[0]), "{growths:?}");
        assert!(growths.iter().all(|&g| g <= 12.0), "{growths:?}");
    }

    #[test]
    fn a_short_run_takes_the_steepest_spiral_and_still_reaches_the_end_radius() {
        let coil = coil_of(&fractions(3, 0.0));
        assert_eq!(coil.growth, 12.0);
        let last = *coil.lines[2].last().unwrap();
        assert!((radius(last) - END).abs() <= 0.5);
    }

    #[test]
    fn the_stroke_is_three_quarters_of_the_turn_spacing_between_4_and_16_px() {
        for fractions in runs() {
            let coil = coil_of(&fractions);
            let spacing = coil.growth * TAU;
            let expected = (0.75 * spacing).clamp(4.0, 16.0);
            let width = f64::from(coil.width);
            assert!(
                (width - expected).abs() <= 0.05 + 1e-6,
                "{width} vs {expected}"
            );
            let tenths = coil.width * 10.0;
            assert_eq!(tenths.round(), tenths, "one decimal: {}", coil.width);
            assert!((4.0..=16.0).contains(&width), "{width}");
            assert!(spacing - width >= 1.0, "turns touch: {spacing} - {width}");
        }
    }

    #[test]
    fn even_110_kilometres_keep_a_4_px_stroke_and_paper_between_the_turns() {
        let coil = coil_of(&fractions(110, 0.0));
        assert!(coil.width >= 4.0, "{}", coil.width);
        assert!(coil.growth * TAU - f64::from(coil.width) >= 1.0);
        assert_eq!(coil.lines.len(), 110);
    }

    #[test]
    fn every_point_stays_inside_the_zone_with_its_stroke() {
        for fractions in runs() {
            let coil = coil_of(&fractions);
            let half = f64::from(coil.width) / 2.0;
            for &(x, y) in coil.lines.iter().flatten() {
                assert!((48.0 + half..=672.0 - half).contains(&x), "x {x}");
                assert!((200.0 + half..=724.0 - half).contains(&y), "y {y}");
            }
        }
    }

    #[test]
    fn the_middle_of_a_split_is_halfway_along_its_share() {
        let coil = coil_of(&fractions(10, 0.0));
        for (i, &middle) in coil.middles.iter().enumerate() {
            let line = &coil.lines[i];
            let (before, after) = (line[0], *line.last().unwrap());
            assert!(radius(middle) >= radius(before) && radius(middle) <= radius(after));
        }
        // On the steepest spiral the radius grows evenly with the angle, and
        // the middle of km 1 lies where the arc is half that of km 1.
        let short = coil_of(&fractions(2, 0.0));
        let target = short.arcs[1] / 2.0;
        // The angle from the radius: km 1 winds past a full turn.
        let middle_angle = (radius(short.middles[0]) - 24.0) / 12.0;
        let along = exact_length(12.0, middle_angle);
        assert!((along - target).abs() < 0.5, "{along} vs {target}");
    }

    #[test]
    fn no_splits_make_no_coil() {
        assert_eq!(coil(&[]), None);
    }

    // ----- build and to_value -----

    fn view(index: u32, fraction: f32) -> SplitView {
        SplitView {
            index,
            pace: "5:00".into(),
            color: format!("#0000{index:02x}"),
            partial: fraction < 1.0,
            fraction,
            t: 0.5,
        }
    }

    fn analysis(views: &[SplitView], fastest: Option<u32>) -> Analysis {
        let splits: Vec<Split> = views
            .iter()
            .map(|v| Split {
                index: v.index,
                distance_m: f64::from(v.fraction) * 1000.0,
                seconds: 300.0 * f64::from(v.fraction),
                partial: v.partial,
            })
            .collect();
        Analysis {
            distance_m: splits.iter().map(|s| s.distance_m).sum(),
            elapsed_s: splits.iter().map(|s| s.seconds).sum(),
            splits,
            fastest,
            start: None,
            points: Vec::new(),
        }
    }

    fn meta() -> Meta {
        Meta {
            runner: "Mara Lindqvist".into(),
            event: "Harbour Half".into(),
            city: None,
            date: time::macros::date!(2026 - 04 - 19),
        }
    }

    fn spiral_of(views: &[SplitView], fastest: Option<u32>) -> Spiral {
        build(
            &analysis(views, fastest),
            views,
            &Palette::for_theme(Theme::Light),
            &TextMeasure::new(),
            &meta(),
        )
    }

    /// A 10.4 km run: ten full kilometres and a 0.4 km partial.
    fn ten_k() -> Vec<SplitView> {
        let mut views: Vec<SplitView> = (1..=10).map(|i| view(i, 1.0)).collect();
        views.push(view(11, 0.4));
        views
    }

    /// The points of SVG path data `M x y L x y ...`.
    fn path_points(path: &str) -> Vec<(f64, f64)> {
        let numbers: Vec<f64> = path.split(' ').filter_map(|t| t.parse().ok()).collect();
        numbers.chunks(2).map(|xy| (xy[0], xy[1])).collect()
    }

    #[test]
    fn every_split_gets_a_segment_in_its_colour_from_its_line() {
        let views = ten_k();
        let spiral = spiral_of(&views, Some(4));
        let coil = coil_of(&fractions(10, f64::from(0.4_f32)));
        assert_eq!(spiral.segments.len(), 11);
        for ((segment, view), line) in spiral.segments.iter().zip(&views).zip(&coil.lines) {
            assert_eq!((segment.index, &segment.color), (view.index, &view.color));
            assert!(segment.path.starts_with("M "), "{}", segment.path);
            let points = path_points(&segment.path);
            assert_eq!(points.len(), line.len(), "km {}", view.index);
            for (drawn, exact) in points.iter().zip(line) {
                assert!((drawn.0 - exact.0).abs() <= 0.051, "{drawn:?} {exact:?}");
                assert!((drawn.1 - exact.1).abs() <= 0.051, "{drawn:?} {exact:?}");
            }
        }
        assert_eq!(spiral.stroke_width, coil.width);
    }

    #[test]
    fn the_start_dot_and_the_finish_square_sit_on_the_ends_of_the_spiral() {
        let spiral = spiral_of(&ten_k(), Some(4));
        let coil = coil_of(&fractions(10, f64::from(0.4_f32)));
        assert_eq!(spiral.start, Some((384.0, 462.0)));
        let (x, y) = *coil.lines[10].last().unwrap();
        assert_eq!(spiral.finish, Some((x as f32, y as f32)));
    }

    #[test]
    fn the_fastest_kilometre_gets_a_ring_around_its_middle_and_its_pace_beside_it() {
        let mut views = ten_k();
        views[3].pace = "4:48".into();
        let spiral = spiral_of(&views, Some(4));
        let coil = coil_of(&fractions(10, f64::from(0.4_f32)));
        let (x, y) = coil.middles[3];
        let callout = spiral.fastest.expect("a callout");
        let ring = ring_at(x as f32, y as f32, coil.width / 2.0 + 4.0);
        assert_eq!(callout.ring, ring);
        let width = TextMeasure::new().width(Role::Mono, "4:48", 12.0);
        assert_eq!(callout.label, label(ring, "4:48".into(), width));
        assert_eq!(callout.label.text, "4:48");
        assert_eq!(spiral_of(&ten_k(), None).fastest, None);
    }

    // ----- the callout -----

    fn ring_at(x: f32, y: f32, r: f32) -> Ring {
        Ring { x, y, r }
    }

    fn assert_label(label: &Label, x: f32, y: f32, anchor: &str) {
        assert!(
            (label.x - x).abs() < 1e-3 && (label.y - y).abs() < 1e-3 && label.anchor == anchor,
            "{label:?}, expected {x} {y} {anchor}"
        );
    }

    #[test]
    fn the_pace_sits_outward_along_the_radius_10_px_beyond_the_ring() {
        // Right of the centre: anchored at its start, the baseline 4.5 px down.
        let right = label(ring_at(500.0, 462.0, 12.0), "4:48".into(), 29.0);
        assert_label(&right, 522.0, 466.5, "start");
        assert_eq!(right.text, "4:48");
        // Left of it: anchored at its end.
        let left = label(ring_at(200.0, 462.0, 12.0), "4:48".into(), 29.0);
        assert_label(&left, 178.0, 466.5, "end");
        // Down and right: 20 px on along the radius through the ring's centre.
        let diagonal = label(ring_at(420.0, 542.0, 10.0), "4:48".into(), 29.0);
        assert_label(&diagonal, 432.0, 562.5, "start");
        // On the centre line: anchored at its start.
        let below = label(ring_at(360.0, 600.0, 10.0), "4:48".into(), 29.0);
        assert_label(&below, 360.0, 624.5, "start");
    }

    #[test]
    fn a_pace_that_would_leave_the_zone_at_the_top_or_bottom_goes_inward() {
        // Outward, the figures would start at y 185.74, above the zone.
        let top = label(ring_at(360.0, 212.0, 12.0), "4:48".into(), 29.0);
        assert_label(&top, 360.0, 238.5, "start");
        // Outward, the baseline would be at y 738.5, below the zone.
        let bottom = label(ring_at(300.0, 712.0, 12.0), "4:48".into(), 29.0);
        let (dx, dy) = (-60.0_f32, 250.0_f32);
        let along = 22.0 / dx.hypot(dy);
        assert_label(&bottom, 300.0 - dx * along, 712.0 - dy * along + 4.5, "end");
        // Just inside: outward.
        let inside = label(ring_at(360.0, 240.0, 12.0), "4:48".into(), 29.0);
        assert_label(&inside, 360.0, 222.5, "start");
    }

    #[test]
    fn a_pace_that_would_leave_the_zone_at_a_side_ends_on_its_edge() {
        // From x 662 it would end at 691: moved left to end at 672.
        let right = label(ring_at(640.0, 462.0, 12.0), "4:48".into(), 29.0);
        assert_label(&right, 643.0, 466.5, "start");
        // Ending at x 58 it would start at 29: moved right to start at 48.
        let left = label(ring_at(80.0, 462.0, 12.0), "4:48".into(), 29.0);
        assert_label(&left, 77.0, 466.5, "end");
    }

    #[test]
    fn every_pace_stays_inside_the_zone_wherever_the_fastest_kilometre_is() {
        let width = TextMeasure::new().width(Role::Mono, "10:48", 12.0);
        for fractions in runs() {
            let coil = coil_of(&fractions);
            for &(x, y) in &coil.middles {
                let ring = ring_at(x as f32, y as f32, coil.width / 2.0 + 4.0);
                let label = label(ring, "10:48".into(), width);
                let left = if label.anchor == "end" {
                    label.x - width
                } else {
                    label.x
                };
                assert!(left >= 48.0 - 1e-3, "{label:?}");
                assert!(left + width <= 672.0 + 1e-3, "{label:?}");
                // The figures are 8.76 px tall at 12 px.
                assert!(label.y - 8.76 >= 200.0 - 1e-3, "{label:?}");
                assert!(label.y <= 724.0 + 1e-3, "{label:?}");
            }
        }
    }

    #[test]
    fn without_splits_the_spiral_is_empty() {
        let spiral = spiral_of(&[], None);
        assert!(spiral.segments.is_empty());
        assert_eq!(
            (spiral.start, spiral.finish, spiral.fastest),
            (None, None, None)
        );
    }

    #[test]
    fn the_template_value_has_every_number_rounded_to_two_decimals() {
        let spiral = Spiral {
            segments: vec![Segment {
                index: 3,
                path: "M 1.0 2.0 L 3.0 4.0".into(),
                color: "#123456".into(),
            }],
            stroke_width: 12.3,
            start: Some((384.0, 462.0)),
            finish: Some((100.123_45, 200.987_65)),
            fastest: Some(Callout {
                ring: ring_at(1.234_5, 2.345_6, 10.000_1),
                label: Label {
                    x: 23.456_7,
                    y: 6.845_6,
                    anchor: "end",
                    text: "4:48".into(),
                },
            }),
        };
        let value = spiral.to_value();
        let get = |path: &str| {
            path.split('.')
                .try_fold(value.clone(), |v, key| v.get_attr(key).ok())
                .unwrap_or_else(|| panic!("no {path}"))
                .to_string()
        };
        assert_eq!(get("stroke_width"), "12.3");
        assert_eq!(get("start.x"), "384");
        assert_eq!(get("start.y"), "462");
        // The square is 6 px wide, centred on the finish.
        assert_eq!(get("finish.x"), "97.12");
        assert_eq!(get("finish.y"), "197.99");
        assert_eq!(get("fastest.x"), "1.23");
        assert_eq!(get("fastest.y"), "2.35");
        assert_eq!(get("fastest.r"), "10");
        assert_eq!(get("fastest.label.x"), "23.46");
        assert_eq!(get("fastest.label.y"), "6.85");
        assert_eq!(get("fastest.label.anchor"), "end");
        assert_eq!(get("fastest.label.text"), "4:48");
        let segment = value
            .get_attr("segments")
            .unwrap()
            .get_item(&Value::from(0))
            .unwrap();
        assert_eq!(
            segment.get_attr("path").unwrap().to_string(),
            "M 1.0 2.0 L 3.0 4.0"
        );
        assert_eq!(segment.get_attr("color").unwrap().to_string(), "#123456");
        assert_eq!(segment.get_attr("index").unwrap().to_string(), "3");
    }

    #[test]
    fn an_empty_spiral_has_no_markers_in_its_value() {
        let value = spiral_of(&[], None).to_value();
        for key in ["start", "finish", "fastest"] {
            assert!(value.get_attr(key).unwrap().is_none(), "{key}");
        }
    }
}
