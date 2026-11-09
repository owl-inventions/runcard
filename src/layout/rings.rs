//! Lays out the `rings` variant: the kilometres as segments of a ring around the finish time.

use minijinja::{context, Value};
use serde::Serialize;

use crate::card::Line;
use crate::format;
use crate::model::{Analysis, Meta, SplitView};
use crate::palette::Palette;
use crate::render::px;
use crate::route::{self, BBox};
use crate::text::{Font, TextMeasure};

/// Centre of the ring in card pixels.
const CENTRE: (f32, f32) = (360.0, 440.0);
/// Outer and inner radius of the ring in card pixels.
const R_OUTER: f32 = 236.0;
const R_INNER: f32 = 150.0;
/// Most angle left blank on each side of a segment, in degrees.
const MAX_GAP_DEG: f32 = 1.6;
/// Share of a segment's angle left blank on each side when that is less
/// than [`MAX_GAP_DEG`], so a long run keeps 64 % of every segment.
const GAP_SHARE: f32 = 0.18;
/// A segment narrower than this once its gaps are taken off is left out, in degrees.
const MIN_SWEEP_DEG: f32 = 0.1;
/// How far outside the outer radius the fastest-km dot sits, in card pixels.
const DOT_OFFSET: f32 = 14.0;
/// The box bottom right that the route outline is fitted into, between the
/// name and the rule, right of the legend and the fastest line.
const ROUTE_BOX: BBox = BBox {
    x: 588.0,
    y: 770.0,
    w: 100.0,
    h: 70.0,
};
/// Point budget of the simplified route outline, as on classic.
const ROUTE_MAX_POINTS: usize = 150;
/// The runner's name: Bebas Neue in capitals, centred under the ring.
const RUNNER_LINE: Line = Line {
    font: Font::Condensed,
    size: 76.0,
    min_size: 40.0,
    max_width: 600.0,
    uppercase: true,
};

/// One kilometre of the ring.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Segment {
    /// 1-based kilometre number.
    pub index: u32,
    /// Where the drawn segment starts and ends, clockwise from 12 o'clock in
    /// degrees, gaps already taken off.
    pub start_deg: f32,
    pub end_deg: f32,
    /// SVG path data of the filled segment.
    pub path: String,
    /// CSS colour of the kilometre's pace.
    pub color: String,
}

/// What `templates/rings.svg` draws besides the shared card data (the time
/// inside the ring is the shared `time`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Rings {
    /// One segment per split, clockwise from 12 o'clock; a split too narrow
    /// to draw has none.
    pub segments: Vec<Segment>,
    /// Centre of the dot outside the fastest kilometre, if there is one.
    pub fastest_dot: Option<(f32, f32)>,
    /// The line under the time: `21.46 KM · 5:43 /KM`.
    pub summary: String,
    /// The runner's name fitted under the ring, and its font size in px.
    pub runner: String,
    pub runner_size: f32,
    /// Route outline in the box bottom right, as SVG path data.
    pub route_path: String,
    /// Start marker of the route.
    pub route_start: (f32, f32),
}

/// Lays out the ring in card pixels.
///
/// Each split gets a share of the ring proportional to its `fraction`, less a
/// gap on each side of [`MAX_GAP_DEG`] or [`GAP_SHARE`] of the share,
/// whichever is smaller; a share too narrow to draw even then is left out.
pub fn build(
    analysis: &Analysis,
    splits: &[SplitView],
    _palette: &Palette,
    measure: &TextMeasure,
    meta: &Meta,
) -> Rings {
    let shares = shares(splits);
    let segments = splits
        .iter()
        .zip(&shares)
        .filter_map(|(view, &(start, end))| {
            let gap = gap(end - start);
            let (start_deg, end_deg) = (start + gap, end - gap);
            (end_deg - start_deg >= MIN_SWEEP_DEG).then(|| Segment {
                index: view.index,
                start_deg,
                end_deg,
                path: arc_segment(CENTRE.0, CENTRE.1, R_OUTER, R_INNER, start_deg, end_deg),
                color: view.color.clone(),
            })
        })
        .collect();
    let fastest_dot = analysis
        .fastest
        .and_then(|index| splits.iter().position(|view| view.index == index))
        .map(|i| {
            let (start, end) = shares[i];
            on_circle(
                CENTRE.0,
                CENTRE.1,
                R_OUTER + DOT_OFFSET,
                (start + end) / 2.0,
            )
        });
    let route = route::outline(&analysis.points, ROUTE_BOX, ROUTE_MAX_POINTS);
    let route_start = route.first().copied().unwrap_or((
        ROUTE_BOX.x + ROUTE_BOX.w / 2.0,
        ROUTE_BOX.y + ROUTE_BOX.h / 2.0,
    ));
    let (runner, runner_size) = RUNNER_LINE.fit(measure, &xml_chars(&meta.runner));
    Rings {
        segments,
        fastest_dot,
        summary: summary(analysis.distance_m, analysis.elapsed_s),
        runner,
        runner_size,
        route_path: route::to_path(&route),
        route_start,
    }
}

/// Start and end angle of every split's share of the ring in degrees,
/// clockwise from 12 o'clock and before the gaps are taken off.
fn shares(splits: &[SplitView]) -> Vec<(f32, f32)> {
    let total: f32 = splits.iter().map(|view| view.fraction).sum();
    let mut covered = 0.0;
    splits
        .iter()
        .map(|view| {
            let start = covered / total * 360.0;
            covered += view.fraction;
            (start, covered / total * 360.0)
        })
        .collect()
}

/// Angle left blank on each side of a segment whose share of the ring is
/// `share_deg`, in degrees.
fn gap(share_deg: f32) -> f32 {
    MAX_GAP_DEG.min(GAP_SHARE * share_deg)
}

/// The point at `radius` from (`cx`, `cy`), `deg` clockwise from 12 o'clock.
fn on_circle(cx: f32, cy: f32, radius: f32, deg: f32) -> (f32, f32) {
    let (sin, cos) = deg.to_radians().sin_cos();
    (cx + radius * sin, cy - radius * cos)
}

/// `21.46 KM · 5:43 /KM`: the distance in two decimals and the average pace.
/// The distance is rounded from the whole metres classic shows, so a
/// marathon's 42.195 km reads 42.20.
fn summary(distance_m: f64, elapsed_s: f64) -> String {
    let pace = format::pace_caps(elapsed_s / distance_m * 1000.0);
    let tens_of_metres = (distance_m.round() / 10.0).round();
    format!("{:.2} KM \u{b7} {pace}", tens_of_metres / 100.0)
}

/// `text` without the characters XML 1.0 forbids, like the shared texts the
/// renderer cleans (`render::xml_chars` is private to it).
fn xml_chars(text: &str) -> String {
    text.chars()
        .filter(|&c| matches!(c, '\t' | '\n' | '\r' | ' '..='\u{fffd}' | '\u{10000}'..))
        .collect()
}

/// SVG path data of a ring segment around (`cx`, `cy`) between `start_deg` and
/// `end_deg`, clockwise from 12 o'clock: the outer arc, a radial line in, the
/// inner arc back and a radial line out. The large-arc flag is set when the
/// segment sweeps more than 180°.
pub fn arc_segment(
    cx: f32,
    cy: f32,
    r_outer: f32,
    r_inner: f32,
    start_deg: f32,
    end_deg: f32,
) -> String {
    let large = u8::from(end_deg - start_deg > 180.0);
    let point = |radius: f32, deg: f32| {
        let (x, y) = on_circle(cx, cy, radius, deg);
        format!("{} {}", px(x), px(y))
    };
    let (ro, ri) = (px(r_outer), px(r_inner));
    format!(
        "M {} A {ro} {ro} 0 {large} 1 {} L {} A {ri} {ri} 0 {large} 0 {} Z",
        point(r_outer, start_deg),
        point(r_outer, end_deg),
        point(r_inner, end_deg),
        point(r_inner, start_deg),
    )
}

impl Rings {
    /// The block as the template's `rings` value, numbers rounded to two decimals.
    pub fn to_value(&self) -> Value {
        let point = |(x, y): (f32, f32)| Value::from(vec![px(x), px(y)]);
        let segments: Vec<Value> = self
            .segments
            .iter()
            .map(|s| context! { index => s.index, path => &s.path, color => &s.color })
            .collect();
        context! {
            segments => segments,
            fastest_dot => self.fastest_dot.map(point),
            summary => &self.summary,
            runner => &self.runner,
            runner_size => px(self.runner_size),
            route_path => &self.route_path,
            route_start => point(self.route_start),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Split, TrackPoint};

    fn view(index: u32, fraction: f32) -> SplitView {
        SplitView {
            index,
            pace: "5:00".into(),
            color: format!("#00000{index}"),
            partial: fraction < 1.0,
            fraction,
            t: 0.5,
        }
    }

    /// Full kilometres numbered from 1, then a partial one of `partial` km if given.
    fn views(full: u32, partial: Option<f32>) -> Vec<SplitView> {
        let mut views: Vec<SplitView> = (1..=full).map(|i| view(i, 1.0)).collect();
        if let Some(fraction) = partial {
            views.push(view(full + 1, fraction));
        }
        views
    }

    fn point(lat: f64, lon: f64) -> TrackPoint {
        TrackPoint {
            lat,
            lon,
            time: None,
        }
    }

    /// An analysis matching `views`, 5:00 per km, with `fastest` as the fastest km.
    fn analysis(views: &[SplitView], fastest: Option<u32>) -> Analysis {
        let splits: Vec<Split> = views
            .iter()
            .map(|v| Split {
                index: v.index,
                distance_m: f64::from(v.fraction) * 1000.0,
                seconds: f64::from(v.fraction) * 300.0,
                partial: v.partial,
            })
            .collect();
        Analysis {
            distance_m: splits.iter().map(|s| s.distance_m).sum(),
            elapsed_s: splits.iter().map(|s| s.seconds).sum(),
            splits,
            fastest,
            start: None,
            points: vec![point(52.5, 13.4), point(52.51, 13.42), point(52.5, 13.43)],
        }
    }

    fn meta(runner: &str) -> Meta {
        Meta {
            runner: runner.into(),
            event: "Spreelauf".into(),
            city: None,
            date: time::macros::date!(2026 - 09 - 27),
        }
    }

    fn rings(views: &[SplitView], analysis: &Analysis, runner: &str) -> Rings {
        build(
            analysis,
            views,
            &Palette::light(),
            &TextMeasure::new(),
            &meta(runner),
        )
    }

    fn rings_of(views: &[SplitView], fastest: Option<u32>) -> Rings {
        rings(views, &analysis(views, fastest), "Julia Brandt")
    }

    fn sweep(segment: &Segment) -> f32 {
        segment.end_deg - segment.start_deg
    }

    fn assert_near(actual: f32, expected: f32, what: &str) {
        assert!(
            (actual - expected).abs() < 1e-3,
            "{what} was {actual}, expected {expected}"
        );
    }

    #[test]
    fn the_segments_fill_the_ring_except_two_gaps_each() {
        let views = views(12, Some(0.46));
        let ring = rings_of(&views, Some(3));
        assert_eq!(ring.segments.len(), 13);
        let total: f32 = ring.segments.iter().map(sweep).sum();
        assert_near(total, 360.0 - 13.0 * 2.0 * MAX_GAP_DEG, "sum of the sweeps");
    }

    #[test]
    fn the_segments_run_clockwise_from_12_o_clock_in_split_order() {
        let ring = rings_of(&views(4, None), Some(2));
        assert_eq!(ring.segments.len(), 4);
        let bounds: Vec<(u32, f32, f32)> = ring
            .segments
            .iter()
            .map(|s| (s.index, s.start_deg, s.end_deg))
            .collect();
        let expected = [
            (1, 0.0, 90.0),
            (2, 90.0, 180.0),
            (3, 180.0, 270.0),
            (4, 270.0, 360.0),
        ];
        for ((index, start, end), (e_index, e_start, e_end)) in bounds.iter().zip(expected) {
            assert_eq!(*index, e_index);
            assert_near(*start, e_start + MAX_GAP_DEG, "start");
            assert_near(*end, e_end - MAX_GAP_DEG, "end");
        }
    }

    #[test]
    fn the_partial_kilometre_spans_its_fraction_of_a_full_one() {
        let ring = rings_of(&views(3, Some(0.5)), Some(1));
        let unit = 360.0 / 3.5;
        assert_near(
            sweep(&ring.segments[0]),
            unit - 2.0 * MAX_GAP_DEG,
            "full km",
        );
        assert_near(
            sweep(&ring.segments[3]),
            0.5 * unit - 2.0 * MAX_GAP_DEG,
            "partial km",
        );
    }

    #[test]
    fn a_single_kilometre_gets_the_whole_ring_but_its_gaps() {
        let ring = rings_of(&views(1, None), Some(1));
        assert_eq!(ring.segments.len(), 1);
        assert_near(ring.segments[0].start_deg, MAX_GAP_DEG, "start");
        assert_near(ring.segments[0].end_deg, 360.0 - MAX_GAP_DEG, "end");
        assert!(
            ring.segments[0].path.contains(" 0 1 1 "),
            "{:?}",
            ring.segments[0]
        );
    }

    #[test]
    fn a_long_run_narrows_every_gap_to_18_percent_of_its_share() {
        // A marathon's kilometre is 360 / 42.195 = 8.53° of the ring: 18 % of
        // it, 1.54°, is less than the 1.6° gap, so every segment keeps 64 %
        // of its share, the 0.195 km partial included.
        let ring = rings_of(&views(42, Some(0.195)), Some(30));
        assert_eq!(ring.segments.len(), 43);
        let unit = 360.0 / 42.195;
        assert_near(sweep(&ring.segments[0]), 0.64 * unit, "full km");
        assert_near(sweep(&ring.segments[42]), 0.64 * 0.195 * unit, "partial");
        let total: f32 = ring.segments.iter().map(sweep).sum();
        assert_near(total, 0.64 * 360.0, "sum of the sweeps");
    }

    #[test]
    fn the_partial_of_a_marathon_is_a_visible_segment_just_before_12_o_clock() {
        let ring = rings_of(&views(42, Some(0.195)), Some(30));
        let partial = ring.segments.last().expect("a segment");
        assert_eq!(partial.index, 43);
        assert!(sweep(partial) > 1.0, "{partial:?}");
        let gap = 0.18 * 0.195 * 360.0 / 42.195;
        assert_near(partial.end_deg, 360.0 - gap, "end of the partial");
    }

    #[test]
    fn a_110_split_run_has_110_segments() {
        // 1.6° on each side would leave 0.07° of a 3.29° kilometre.
        let ring = rings_of(&views(109, Some(0.4)), Some(1));
        assert_eq!(ring.segments.len(), 110);
        let unit = 360.0 / 109.4;
        for segment in &ring.segments[..109] {
            assert_near(
                sweep(segment),
                0.64 * unit,
                &format!("km {}", segment.index),
            );
        }
    }

    #[test]
    fn a_segment_too_narrow_to_draw_is_left_out() {
        // A metre of a 3 km run is 0.12° of the ring, 0.08° once its gaps
        // are taken off.
        let ring = rings_of(&views(3, Some(0.001)), Some(1));
        assert_eq!(ring.segments.len(), 3);
        assert!(ring.segments.iter().all(|s| sweep(s) > 0.0), "{ring:?}");
    }

    #[test]
    fn each_segment_is_filled_in_its_kilometre_colour() {
        let views = views(4, None);
        let ring = rings_of(&views, Some(2));
        assert_eq!(ring.segments.len(), 4);
        for (segment, view) in ring.segments.iter().zip(&views) {
            assert_eq!(segment.color, view.color);
            assert_eq!(
                segment.path,
                arc_segment(
                    360.0,
                    440.0,
                    236.0,
                    150.0,
                    segment.start_deg,
                    segment.end_deg
                )
            );
        }
    }

    #[test]
    fn the_fastest_dot_sits_14_px_outside_the_ring_at_the_mid_angle() {
        let ring = rings_of(&views(4, None), Some(2));
        let (x, y) = ring.fastest_dot.expect("a fastest dot");
        let r = 236.0 + 14.0;
        let mid = 135_f32.to_radians();
        assert_near(x, 360.0 + r * mid.sin(), "dot x");
        assert_near(y, 440.0 - r * mid.cos(), "dot y");
    }

    #[test]
    fn without_a_fastest_kilometre_there_is_no_dot() {
        assert_eq!(rings_of(&views(0, Some(0.6)), None).fastest_dot, None);
    }

    #[test]
    fn arc_segment_draws_both_arcs_and_closes() {
        let path = arc_segment(100.0, 100.0, 50.0, 30.0, 0.0, 90.0);
        assert_eq!(
            path,
            "M 100.0 50.0 A 50.0 50.0 0 0 1 150.0 100.0 L 130.0 100.0 A 30.0 30.0 0 0 0 100.0 70.0 Z"
        );
    }

    #[test]
    fn arc_segment_sets_the_large_arc_flag_only_above_180_degrees() {
        let flags = |end: f32| {
            let path = arc_segment(360.0, 440.0, 236.0, 150.0, 10.0, end);
            let outer = path.split(" A ").nth(1).unwrap().to_owned();
            let inner = path.split(" A ").nth(2).unwrap().to_owned();
            (outer, inner)
        };
        let (outer, inner) = flags(190.0);
        assert!(outer.starts_with("236.0 236.0 0 0 1 "), "{outer}");
        assert!(inner.starts_with("150.0 150.0 0 0 0 "), "{inner}");
        let (outer, inner) = flags(190.5);
        assert!(outer.starts_with("236.0 236.0 0 1 1 "), "{outer}");
        assert!(inner.starts_with("150.0 150.0 0 1 0 "), "{inner}");
    }

    #[test]
    fn the_summary_has_the_distance_in_two_decimals_and_the_average_pace() {
        let views = views(21, Some(0.46));
        let analysis = Analysis {
            distance_m: 21_460.0,
            elapsed_s: 2.0 * 3600.0 + 2.0 * 60.0 + 37.0,
            ..analysis(&views, Some(1))
        };
        let ring = rings(&views, &analysis, "Julia Brandt");
        assert_eq!(ring.summary, "21.46 KM \u{b7} 5:43 /KM");
    }

    #[test]
    fn the_summary_rounds_the_distance_as_classic_shows_it() {
        // Classic shows 42.195 km, so the ring says 42.20, not the 42.19 that
        // the binary 42.19499... would round to.
        for (distance_m, expected) in [
            (42_195.0, "42.20"),
            (42_194.7, "42.20"),
            (42_194.4, "42.19"),
        ] {
            let summary = summary(distance_m, 4.0 * 3600.0);
            assert!(
                summary.starts_with(&format!("{expected} KM ")),
                "{distance_m}: {summary}"
            );
        }
    }

    #[test]
    fn a_normal_name_keeps_its_full_size() {
        let ring = rings_of(&views(4, None), Some(2));
        assert_eq!(
            (ring.runner.as_str(), ring.runner_size),
            ("Julia Brandt", 76.0)
        );
    }

    #[test]
    fn a_long_name_is_shrunk_to_fit_600_px() {
        let name = "Maximiliane Wittgenstein-Hohenlohe";
        let ring = rings(&views(4, None), &analysis(&views(4, None), Some(2)), name);
        let measure = TextMeasure::new();
        let width = |size: f32| measure.width(Font::Condensed, &name.to_uppercase(), size, 0.0);
        assert_eq!(ring.runner, name);
        assert!(ring.runner_size < 76.0, "size {}", ring.runner_size);
        assert!(
            width(ring.runner_size) <= 600.0,
            "{}",
            width(ring.runner_size)
        );
        assert!(width(ring.runner_size + 0.1) > 600.0);
    }

    #[test]
    fn characters_xml_forbids_are_dropped_from_the_name() {
        let ring = rings(
            &views(4, None),
            &analysis(&views(4, None), Some(2)),
            "Julia\u{1} Brandt",
        );
        assert_eq!(ring.runner, "Julia Brandt");
    }

    #[test]
    fn the_route_is_fitted_into_the_box_bottom_right() {
        let ring = rings_of(&views(4, None), Some(2));
        assert!(ring.route_path.starts_with("M "), "{}", ring.route_path);
        let numbers: Vec<f32> = ring
            .route_path
            .split(' ')
            .filter_map(|t| t.parse().ok())
            .collect();
        for xy in numbers.chunks(2) {
            assert!((588.0..=688.0).contains(&xy[0]), "x {}", xy[0]);
            assert!((770.0..=840.0).contains(&xy[1]), "y {}", xy[1]);
        }
        // The path is written with one decimal.
        assert!(
            (ring.route_start.0 - numbers[0]).abs() <= 0.05,
            "{:?}",
            ring.route_start
        );
        assert!(
            (ring.route_start.1 - numbers[1]).abs() <= 0.05,
            "{:?}",
            ring.route_start
        );
    }

    #[test]
    fn the_route_box_keeps_clear_of_the_name_the_fastest_line_and_the_rule() {
        // From y = 770 to 840 the box starts 30 px under the name's baseline
        // at y = 740 and ends 20 px above the rule at y = 860.
        assert_eq!(
            (ROUTE_BOX.x, ROUTE_BOX.y, ROUTE_BOX.w, ROUTE_BOX.h),
            (588.0, 770.0, 100.0, 70.0)
        );
        // The fastest line starts at x = 32 in Plex Mono 13 px with 3 px
        // letter spacing; the longest plausible one ends 8 px before the box.
        let line = format::fastest_line(100, 600.0);
        assert_eq!(line, "FASTEST KM 100 \u{b7} 10:00 /KM");
        let end = 32.0 + TextMeasure::new().width(Font::Mono, &line, 13.0, 3.0);
        assert!(end < ROUTE_BOX.x - 8.0, "the fastest line ends at {end}");
    }

    #[test]
    fn the_value_rounds_the_numbers_for_the_template() {
        let ring = Rings {
            segments: vec![Segment {
                index: 2,
                start_deg: 91.6,
                end_deg: 178.4,
                path: "M 1 2 Z".into(),
                color: "#e8601c".into(),
            }],
            fastest_dot: Some((536.776_7, 616.776_7)),
            summary: "21.46 KM \u{b7} 5:43 /KM".into(),
            runner: "Julia Brandt".into(),
            runner_size: 61.300_003,
            route_path: "M 600 800 L 610 810".into(),
            route_start: (600.004, 800.0),
        };
        let value = ring.to_value();
        let attr = |value: &Value, name: &str| value.get_attr(name).unwrap();
        let segment = attr(&value, "segments").get_item(&Value::from(0)).unwrap();
        assert_eq!(attr(&segment, "index").to_string(), "2");
        assert_eq!(attr(&segment, "path").as_str(), Some("M 1 2 Z"));
        assert_eq!(attr(&segment, "color").as_str(), Some("#e8601c"));
        assert_eq!(attr(&value, "fastest_dot").to_string(), "[536.78, 616.78]");
        assert_eq!(
            attr(&value, "summary").as_str(),
            Some("21.46 KM \u{b7} 5:43 /KM")
        );
        assert_eq!(attr(&value, "runner").as_str(), Some("Julia Brandt"));
        assert_eq!(attr(&value, "runner_size").to_string(), "61.3");
        assert_eq!(
            attr(&value, "route_path").as_str(),
            Some("M 600 800 L 610 810")
        );
        assert_eq!(attr(&value, "route_start").to_string(), "[600.0, 800.0]");
    }
}
