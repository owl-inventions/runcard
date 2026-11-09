//! Lays out the `route` variant: the route drawn large, one stroke per kilometre.

use minijinja::{context, Value};
use serde::Serialize;

use crate::card::{self, Line, STRIP_WIDTH};
use crate::geo;
use crate::model::{Analysis, Meta, SplitView};
use crate::palette::Palette;
use crate::render::px;
use crate::route::{self, BBox};
use crate::text::{Font, TextMeasure};

/// The box the route is fitted into, in card pixels.
const MAP_BOX: BBox = BBox {
    x: 60.0,
    y: 170.0,
    w: 600.0,
    h: 430.0,
};
/// Most points the route is drawn with; a longer track keeps every n-th one.
const MAX_POINTS: usize = 4000;
/// Metres in a kilometre.
const KM: f64 = 1000.0;
/// The runner's name: Bebas Neue in capitals across the width of the card.
const RUNNER_LINE: Line = Line {
    font: Font::Condensed,
    size: 76.0,
    min_size: 40.0,
    max_width: STRIP_WIDTH,
    uppercase: true,
};

/// One kilometre of the route.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Segment {
    /// 1-based kilometre number.
    pub index: u32,
    /// SVG path data of the kilometre's polyline.
    pub path: String,
    /// CSS colour of the kilometre's pace.
    pub color: String,
}

/// One stripe of the compact bar under the map.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Band {
    /// Left edge and width in card pixels.
    pub x: f32,
    pub width: f32,
    /// CSS colour of the kilometre's pace.
    pub color: String,
}

/// What `templates/route.svg` draws besides the shared card data.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RouteHero {
    /// One polyline per kilometre, each ending where the next one starts;
    /// empty when the track has fewer than two distinct points.
    pub segments: Vec<Segment>,
    /// First and last point of the drawn route.
    pub start: Option<(f32, f32)>,
    pub finish: Option<(f32, f32)>,
    /// The stripe bar under the map, one band per split.
    pub strip: Vec<Band>,
    /// Horizontal position of the fastest-km dot over the bar.
    pub fastest_x: Option<f32>,
    /// The runner's name fitted across the card, and its font size in px.
    pub runner: String,
    pub runner_size: f32,
}

/// Lays out the route map in card pixels.
///
/// Every track point is projected and fitted into [`MAP_BOX`], repeated
/// points are dropped and a track longer than [`MAX_POINTS`] is thinned out;
/// the points are then cut into kilometres by their running distance.
pub fn build(
    analysis: &Analysis,
    splits: &[SplitView],
    _palette: &Palette,
    measure: &TextMeasure,
    meta: &Meta,
) -> RouteHero {
    let fitted = route::fit(&route::project(&analysis.points), MAP_BOX);
    let mut track: Vec<((f32, f32), f64)> = fitted
        .into_iter()
        .zip(geo::cumulative_m(&analysis.points))
        .collect();
    track.dedup_by(|next, kept| next.0 == kept.0);
    let (points, distances): (Vec<_>, Vec<_>) = if track.len() < 2 {
        (Vec::new(), Vec::new())
    } else {
        decimate(track, MAX_POINTS).into_iter().unzip()
    };
    let segments = per_km(&points, &distances, splits.len())
        .iter()
        .zip(splits)
        .map(|(points, view)| Segment {
            index: view.index,
            path: route::to_path(points),
            color: view.color.clone(),
        })
        .collect();
    let spans = card::spans(&analysis.splits);
    let strip = spans
        .iter()
        .zip(splits)
        .map(|(span, view)| Band {
            x: span.x,
            width: span.width,
            color: view.color.clone(),
        })
        .collect();
    let fastest_x = analysis
        .fastest
        .and_then(|index| analysis.splits.iter().position(|s| s.index == index))
        .map(|i| spans[i].centre());
    let (runner, runner_size) = RUNNER_LINE.fit(measure, &xml_chars(&meta.runner));
    RouteHero {
        segments,
        start: points.first().copied(),
        finish: points.last().copied(),
        strip,
        fastest_x,
        runner,
        runner_size,
    }
}

/// The polyline of every kilometre: `points` cut where their running distance
/// `distances` crosses a whole kilometre, at a point interpolated on the
/// crossing step, which ends one kilometre and starts the next. Points beyond
/// the last of `kms` kilometres go to the last one.
fn per_km(points: &[(f32, f32)], distances: &[f64], kms: usize) -> Vec<Vec<(f32, f32)>> {
    let Some(&first) = points.first().filter(|_| kms > 0) else {
        return Vec::new();
    };
    let mut segments = vec![vec![first]];
    for i in 1..points.len() {
        // Every crossing up to the previous point is done, so a crossing at or
        // before this point lies after the previous one.
        while segments.len() < kms {
            let boundary = segments.len() as f64 * KM;
            if distances[i] < boundary {
                break;
            }
            let along = ((boundary - distances[i - 1]) / (distances[i] - distances[i - 1])) as f32;
            let crossing = lerp(points[i - 1], points[i], along);
            push_new(&mut segments, crossing);
            segments.push(vec![crossing]);
        }
        push_new(&mut segments, points[i]);
    }
    segments
}

/// Appends `point` to the last segment unless it already ends there.
fn push_new(segments: &mut [Vec<(f32, f32)>], point: (f32, f32)) {
    if let Some(segment) = segments.last_mut() {
        if segment.last() != Some(&point) {
            segment.push(point);
        }
    }
}

/// The point `along` (0 to 1) of the way from `a` to `b`.
fn lerp(a: (f32, f32), b: (f32, f32), along: f32) -> (f32, f32) {
    (a.0 + (b.0 - a.0) * along, a.1 + (b.1 - a.1) * along)
}

/// At most `max` of `points`, every n-th one, the first and the last always kept.
fn decimate<T: Copy>(points: Vec<T>, max: usize) -> Vec<T> {
    let Some(&last) = points.last().filter(|_| points.len() > max) else {
        return points;
    };
    // Points 0, n, 2n, ... up to the last one, plus the last one if n misses it.
    let step = (points.len() - 1).div_ceil(max.max(2) - 1);
    let mut kept: Vec<T> = points.iter().step_by(step).copied().collect();
    if !(points.len() - 1).is_multiple_of(step) {
        kept.push(last);
    }
    kept
}

/// `text` without the characters XML 1.0 forbids, like the shared texts the
/// renderer cleans (`render::xml_chars` is private to it).
fn xml_chars(text: &str) -> String {
    text.chars()
        .filter(|&c| matches!(c, '\t' | '\n' | '\r' | ' '..='\u{fffd}' | '\u{10000}'..))
        .collect()
}

impl RouteHero {
    /// The block as the template's `route_hero` value, numbers rounded to two decimals.
    pub fn to_value(&self) -> Value {
        let point = |(x, y): (f32, f32)| Value::from(vec![px(x), px(y)]);
        let segments: Vec<Value> = self
            .segments
            .iter()
            .map(|s| context! { index => s.index, path => &s.path, color => &s.color })
            .collect();
        let strip: Vec<Value> = self
            .strip
            .iter()
            .map(|b| context! { x => px(b.x), width => px(b.width), color => &b.color })
            .collect();
        context! {
            segments => segments,
            start => self.start.map(point),
            finish => self.finish.map(point),
            strip => strip,
            fastest_x => self.fastest_x.map(px),
            runner => &self.runner,
            runner_size => px(self.runner_size),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::EARTH_RADIUS_M;
    use crate::model::{Split, TrackPoint};

    const LAT: f64 = 52.5;
    const LON: f64 = 13.4;

    /// The point `east` and `north` metres from 52.5 N 13.4 E.
    fn point(east: f64, north: f64) -> TrackPoint {
        let per_degree = EARTH_RADIUS_M.to_radians();
        TrackPoint {
            lat: LAT + north / per_degree,
            lon: LON + east / (per_degree * LAT.to_radians().cos()),
            time: None,
        }
    }

    /// An L-shaped track, `east_steps` steps of 100 m east then
    /// `north_steps` north, with full and partial splits by its measured length.
    fn track(east_steps: u32, north_steps: u32) -> Vec<TrackPoint> {
        let east = (0..=east_steps).map(|i| point(f64::from(i) * 100.0, 0.0));
        let north =
            (1..=north_steps).map(|i| point(f64::from(east_steps) * 100.0, f64::from(i) * 100.0));
        east.chain(north).collect()
    }

    /// The analysis of `points` at 5:00 per km, km 2 the fastest if there is one.
    fn analysis(points: Vec<TrackPoint>) -> Analysis {
        let total = geo::cumulative_m(&points).last().copied().unwrap_or(0.0);
        let full = (total / 1000.0).floor() as u32;
        let mut splits: Vec<Split> = (1..=full)
            .map(|index| Split {
                index,
                distance_m: 1000.0,
                seconds: if index == 2 { 280.0 } else { 300.0 },
                partial: false,
            })
            .collect();
        let rest = total - f64::from(full) * 1000.0;
        if rest > 1.0 {
            splits.push(Split {
                index: full + 1,
                distance_m: rest,
                seconds: rest * 0.3,
                partial: true,
            });
        }
        Analysis {
            distance_m: total,
            elapsed_s: splits.iter().map(|s| s.seconds).sum(),
            fastest: (full >= 2).then_some(2),
            splits,
            start: None,
            points,
        }
    }

    fn views(analysis: &Analysis) -> Vec<SplitView> {
        analysis
            .splits
            .iter()
            .map(|split| SplitView {
                index: split.index,
                pace: "5:00".into(),
                color: format!("#0000{:02x}", split.index),
                partial: split.partial,
                fraction: (split.distance_m / 1000.0) as f32,
                t: 0.5,
            })
            .collect()
    }

    fn meta(runner: &str) -> Meta {
        Meta {
            runner: runner.into(),
            event: "Spreelauf".into(),
            city: None,
            date: time::macros::date!(2026 - 09 - 27),
        }
    }

    fn hero_for(analysis: &Analysis, runner: &str) -> RouteHero {
        build(
            analysis,
            &views(analysis),
            &Palette::light(),
            &TextMeasure::new(),
            &meta(runner),
        )
    }

    fn hero(analysis: &Analysis) -> RouteHero {
        hero_for(analysis, "Julia Brandt")
    }

    /// The points of SVG path data `M x y L x y ...`.
    fn path_points(path: &str) -> Vec<(f32, f32)> {
        let numbers: Vec<f32> = path.split(' ').filter_map(|t| t.parse().ok()).collect();
        numbers.chunks(2).map(|xy| (xy[0], xy[1])).collect()
    }

    fn assert_close(actual: (f32, f32), expected: (f32, f32), what: &str) {
        assert!(
            (actual.0 - expected.0).abs() <= 0.05 && (actual.1 - expected.1).abs() <= 0.05,
            "{what} was {actual:?}, expected {expected:?}"
        );
    }

    #[test]
    fn every_point_lands_in_one_kilometre_and_neighbours_share_the_crossing() {
        let points: Vec<(f32, f32)> = (0..7).map(|i| (i as f32, 0.0)).collect();
        let distances = [0.0, 400.0, 800.0, 1200.0, 1600.0, 2000.0, 2300.0];
        let kms = per_km(&points, &distances, 3);
        assert_eq!(
            kms,
            vec![
                vec![(0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (2.5, 0.0)],
                vec![(2.5, 0.0), (3.0, 0.0), (4.0, 0.0), (5.0, 0.0)],
                vec![(5.0, 0.0), (6.0, 0.0)],
            ]
        );
    }

    #[test]
    fn a_kilometre_without_points_of_its_own_still_gets_its_stretch() {
        let points = [(0.0, 0.0), (25.0, 0.0)];
        let kms = per_km(&points, &[0.0, 2500.0], 3);
        assert_eq!(
            kms,
            vec![
                vec![(0.0, 0.0), (10.0, 0.0)],
                vec![(10.0, 0.0), (20.0, 0.0)],
                vec![(20.0, 0.0), (25.0, 0.0)],
            ]
        );
    }

    #[test]
    fn points_beyond_the_last_kilometre_go_to_the_last_one() {
        let points: Vec<(f32, f32)> = (0..5).map(|i| (i as f32, 0.0)).collect();
        let kms = per_km(&points, &[0.0, 600.0, 1000.5, 1000.8, 1001.0], 1);
        assert_eq!(kms, vec![points.clone()]);
    }

    #[test]
    fn decimation_keeps_at_most_the_budget_with_the_first_and_last_point() {
        for len in [4001, 4002, 7999, 10_001, 12_345] {
            let points: Vec<usize> = (0..len).collect();
            let kept = decimate(points, 4000);
            assert!(kept.len() <= 4000, "{len}: {} kept", kept.len());
            assert!(kept.len() > 1500, "{len}: only {} kept", kept.len());
            assert_eq!(kept.first(), Some(&0), "{len}");
            assert_eq!(kept.last(), Some(&(len - 1)), "{len}");
            let steps: Vec<usize> = kept.windows(2).map(|w| w[1] - w[0]).collect();
            let step = steps[0];
            assert!(
                steps.iter().all(|&s| s > 0 && s <= step),
                "{len}: {steps:?}"
            );
        }
    }

    #[test]
    fn a_track_within_the_budget_is_not_decimated() {
        let points: Vec<usize> = (0..4000).collect();
        assert_eq!(decimate(points.clone(), 4000), points);
    }

    #[test]
    fn every_split_gets_a_segment_in_its_colour() {
        let analysis = analysis(track(25, 9));
        let views = views(&analysis);
        assert_eq!(views.len(), 4, "3.4 km: three full kms and a partial");
        let hero = hero(&analysis);
        assert_eq!(hero.segments.len(), views.len());
        for (segment, view) in hero.segments.iter().zip(&views) {
            assert_eq!((segment.index, &segment.color), (view.index, &view.color));
        }
    }

    #[test]
    fn neighbouring_segments_share_their_boundary_point() {
        let hero = hero(&analysis(track(25, 9)));
        assert_eq!(hero.segments.len(), 4);
        for pair in hero.segments.windows(2) {
            let (before, after) = (path_points(&pair[0].path), path_points(&pair[1].path));
            assert!(before.len() >= 2 && after.len() >= 2, "{pair:?}");
            assert_eq!(before.last(), after.first(), "{pair:?}");
        }
    }

    #[test]
    fn the_markers_are_the_first_and_last_fitted_point() {
        let analysis = analysis(track(25, 9));
        let fitted = route::fit(&route::project(&analysis.points), MAP_BOX);
        let hero = hero(&analysis);
        assert_eq!(hero.start, fitted.first().copied());
        assert_eq!(hero.finish, fitted.last().copied());
        let first = path_points(&hero.segments[0].path)[0];
        let last = *path_points(&hero.segments[3].path).last().unwrap();
        assert_close(first, fitted[0], "path start");
        assert_close(last, fitted[fitted.len() - 1], "path end");
    }

    #[test]
    fn the_route_is_fitted_into_the_map_box() {
        let hero = hero(&analysis(track(25, 9)));
        let points: Vec<(f32, f32)> = hero
            .segments
            .iter()
            .flat_map(|s| path_points(&s.path))
            .collect();
        for &(x, y) in &points {
            assert!((59.95..=660.05).contains(&x), "x {x}");
            assert!((169.95..=600.05).contains(&y), "y {y}");
        }
        // 2.5 km east, 0.9 km north: as wide as the box.
        let min_x = points.iter().map(|p| p.0).fold(f32::MAX, f32::min);
        let max_x = points.iter().map(|p| p.0).fold(f32::MIN, f32::max);
        assert_close((min_x, max_x), (60.0, 660.0), "horizontal extent");
    }

    #[test]
    fn repeated_points_are_drawn_once() {
        let once = analysis(track(25, 9));
        let doubled = Analysis {
            points: once
                .points
                .iter()
                .flat_map(|p| [p.clone(), p.clone()])
                .collect(),
            ..once.clone()
        };
        let drawn = hero(&once);
        assert_eq!(drawn.segments.len(), 4);
        assert_eq!(hero(&doubled), drawn);
    }

    #[test]
    fn a_long_track_is_drawn_with_at_most_4000_points() {
        // 9000 points 1 m apart: 9 km.
        let points: Vec<TrackPoint> = (0..9000).map(|i| point(f64::from(i), 0.0)).collect();
        let analysis = analysis(points);
        let hero = hero(&analysis);
        assert_eq!(hero.segments.len(), analysis.splits.len());
        let drawn: usize = hero
            .segments
            .iter()
            .map(|s| path_points(&s.path).len())
            .sum();
        // Each crossing is drawn twice, at the end of one km and the start of the next.
        let shared = 2 * (hero.segments.len() - 1);
        assert!(drawn <= 4000 + shared, "{drawn} points drawn");
        assert!(drawn > 2000, "{drawn} points drawn");
    }

    #[test]
    fn a_track_without_two_distinct_points_draws_no_route() {
        let analysis = Analysis {
            points: vec![point(0.0, 0.0), point(0.0, 0.0)],
            ..analysis(track(25, 9))
        };
        let empty = hero(&analysis);
        assert!(empty.segments.is_empty(), "{empty:?}");
        assert_eq!((empty.start, empty.finish), (None, None));
        let single = Analysis {
            points: vec![point(0.0, 0.0)],
            ..analysis.clone()
        };
        assert!(hero(&single).segments.is_empty());
    }

    #[test]
    fn the_strip_has_a_band_per_split_laid_out_as_classic() {
        let analysis = analysis(track(25, 9));
        let views = views(&analysis);
        let hero = hero(&analysis);
        let spans = card::spans(&analysis.splits);
        assert_eq!(hero.strip.len(), 4);
        for ((band, span), view) in hero.strip.iter().zip(&spans).zip(&views) {
            assert_eq!((band.x, band.width), (span.x, span.width));
            assert_eq!(band.color, view.color);
        }
        assert_eq!(hero.fastest_x, Some(spans[1].centre()));
    }

    #[test]
    fn without_a_fastest_kilometre_there_is_no_dot() {
        let analysis = Analysis {
            fastest: None,
            ..analysis(track(5, 0))
        };
        assert_eq!(hero(&analysis).fastest_x, None);
    }

    #[test]
    fn a_long_name_is_shrunk_to_fit_the_card_width() {
        let name = "Maximiliane Feodora Wittgenstein-Hohenlohe";
        let hero = hero_for(&analysis(track(25, 9)), name);
        let measure = TextMeasure::new();
        let width = |size: f32| measure.width(Font::Condensed, &name.to_uppercase(), size, 0.0);
        assert_eq!(hero.runner, name);
        assert!(hero.runner_size < 76.0, "size {}", hero.runner_size);
        assert!(
            width(hero.runner_size) <= 656.0,
            "{}",
            width(hero.runner_size)
        );
        assert!(width(hero.runner_size + 0.1) > 656.0);
        let short = hero_for(&analysis(track(25, 9)), "Julia\u{1} Brandt");
        assert_eq!(
            (short.runner.as_str(), short.runner_size),
            ("Julia Brandt", 76.0)
        );
    }

    #[test]
    fn the_value_rounds_the_numbers_for_the_template() {
        let hero = RouteHero {
            segments: vec![Segment {
                index: 3,
                path: "M 60.0 170.0 L 70.0 180.0".into(),
                color: "#e8601c".into(),
            }],
            start: Some((60.004, 170.0)),
            finish: Some((70.0, 180.006)),
            strip: vec![Band {
                x: 32.0,
                width: 13.302_326,
                color: "#2c4a5e".into(),
            }],
            fastest_x: Some(38.651_16),
            runner: "Julia Brandt".into(),
            runner_size: 61.300_003,
        };
        let value = hero.to_value();
        let attr = |value: &Value, name: &str| value.get_attr(name).unwrap();
        let segment = attr(&value, "segments").get_item(&Value::from(0)).unwrap();
        assert_eq!(attr(&segment, "index").to_string(), "3");
        assert_eq!(
            attr(&segment, "path").as_str(),
            Some("M 60.0 170.0 L 70.0 180.0")
        );
        assert_eq!(attr(&segment, "color").as_str(), Some("#e8601c"));
        assert_eq!(attr(&value, "start").to_string(), "[60.0, 170.0]");
        assert_eq!(attr(&value, "finish").to_string(), "[70.0, 180.01]");
        let band = attr(&value, "strip").get_item(&Value::from(0)).unwrap();
        assert_eq!(attr(&band, "x").to_string(), "32.0");
        assert_eq!(attr(&band, "width").to_string(), "13.3");
        assert_eq!(attr(&band, "color").as_str(), Some("#2c4a5e"));
        assert_eq!(attr(&value, "fastest_x").to_string(), "38.65");
        assert_eq!(attr(&value, "runner").as_str(), Some("Julia Brandt"));
        assert_eq!(attr(&value, "runner_size").to_string(), "61.3");
    }
}
