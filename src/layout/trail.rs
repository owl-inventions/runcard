//! Lays out the `trail` variant: the route, split into one stroke per kilometre.

use minijinja::{context, Value};
use serde::Serialize;

use crate::geo;
use crate::model::{Analysis, Meta, SplitView};
use crate::palette::Palette;
use crate::render::px;
use crate::route::{self, BBox};
use crate::text::TextMeasure;

/// The visualisation zone inset by 24 px: the route is fitted into it.
const TRAIL_BOX: BBox = BBox {
    x: 72.0,
    y: 184.0,
    w: 576.0,
    h: 512.0,
};
/// Most points the route is drawn with; a longer track keeps every n-th one.
const MAX_POINTS: usize = 4000;
/// Metres in a kilometre.
const KM: f64 = 1000.0;
/// Half the side of the finish square.
const FINISH_HALF: f32 = 3.5;

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

/// What `templates/trail.svg` draws inside the visualisation zone.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Trail {
    /// One polyline per split, each starting where the one before ends;
    /// empty when the track has fewer than two distinct points.
    pub segments: Vec<Segment>,
    /// Centre of the start circle, the first point of the route.
    pub start: Option<(f32, f32)>,
    /// Centre of the finish square, the last point of the route.
    pub finish: Option<(f32, f32)>,
}

/// Lays out the trail in card pixels.
///
/// Every track point is projected and fitted into [`TRAIL_BOX`], repeated
/// points are dropped and a track longer than [`MAX_POINTS`] is thinned out;
/// the points are then cut into kilometres by their running distance.
pub fn build(
    analysis: &Analysis,
    splits: &[SplitView],
    _palette: &Palette,
    _measure: &TextMeasure,
    _meta: &Meta,
) -> Trail {
    let fitted = route::fit(&route::project(&analysis.points), TRAIL_BOX);
    let mut track: Vec<((f32, f32), f64)> = fitted
        .into_iter()
        .zip(geo::cumulative_m(&analysis.points))
        .collect();
    track.dedup_by(|next, kept| next.0 == kept.0);
    if track.len() < 2 {
        return Trail {
            segments: Vec::new(),
            start: None,
            finish: None,
        };
    }
    let (points, distances): (Vec<_>, Vec<_>) = decimate(track, MAX_POINTS).into_iter().unzip();
    let segments = per_km(&points, &distances, splits.len())
        .iter()
        .zip(splits)
        .map(|(points, view)| Segment {
            index: view.index,
            path: route::to_path(points),
            color: view.color.clone(),
        })
        .collect();
    Trail {
        segments,
        start: points.first().copied(),
        finish: points.last().copied(),
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

impl Trail {
    /// The block as the template sees it, under the key `trail`: the finish
    /// as the top-left corner of its 7 px square, every number rounded to
    /// two decimals.
    pub fn to_value(&self) -> Value {
        let segments: Vec<Value> = self
            .segments
            .iter()
            .map(|s| context! { index => s.index, path => &s.path, color => &s.color })
            .collect();
        context! {
            segments => segments,
            start => self.start.map(|(x, y)| context! { x => px(x), y => px(y) }),
            finish => self
                .finish
                .map(|(x, y)| context! { x => px(x - FINISH_HALF), y => px(y - FINISH_HALF) }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::{self, EARTH_RADIUS_M};
    use crate::model::{Split, TrackPoint};
    use crate::route;
    use crate::theme::Theme;

    const LAT: f64 = 38.7;
    const LON: f64 = -9.14;

    /// The point `east` and `north` metres from 38.7 N 9.14 W.
    fn point(east: f64, north: f64) -> TrackPoint {
        let per_degree = EARTH_RADIUS_M.to_radians();
        TrackPoint {
            lat: LAT + north / per_degree,
            lon: LON + east / (per_degree * LAT.to_radians().cos()),
            time: None,
        }
    }

    /// An L-shaped track, `east_steps` steps of 100 m east then
    /// `north_steps` north.
    fn track(east_steps: u32, north_steps: u32) -> Vec<TrackPoint> {
        let east = (0..=east_steps).map(|i| point(f64::from(i) * 100.0, 0.0));
        let north =
            (1..=north_steps).map(|i| point(f64::from(east_steps) * 100.0, f64::from(i) * 100.0));
        east.chain(north).collect()
    }

    /// The analysis of `points` at 5:00 per km, with full and partial splits
    /// by its measured length, km 2 the fastest if there is one.
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

    fn meta() -> Meta {
        Meta {
            runner: "Mara Lindqvist".into(),
            event: "Harbour Half".into(),
            city: None,
            date: time::macros::date!(2026 - 04 - 19),
        }
    }

    fn trail_of(analysis: &Analysis) -> Trail {
        build(
            analysis,
            &views(analysis),
            &Palette::for_theme(Theme::Light),
            &TextMeasure::new(),
            &meta(),
        )
    }

    /// The points of SVG path data `M x y L x y ...`.
    fn path_points(path: &str) -> Vec<(f32, f32)> {
        let numbers: Vec<f32> = path.split(' ').filter_map(|t| t.parse().ok()).collect();
        numbers.chunks(2).map(|xy| (xy[0], xy[1])).collect()
    }

    fn extent(points: &[(f32, f32)]) -> (f32, f32, f32, f32) {
        points.iter().fold(
            (f32::MAX, f32::MAX, f32::MIN, f32::MIN),
            |(x0, y0, x1, y1), &(x, y)| (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
        )
    }

    fn all_points(trail: &Trail) -> Vec<(f32, f32)> {
        trail
            .segments
            .iter()
            .flat_map(|s| path_points(&s.path))
            .collect()
    }

    fn assert_near(actual: f32, expected: f32, what: &str) {
        assert!(
            (actual - expected).abs() <= 0.15,
            "{what} was {actual}, expected {expected}"
        );
    }

    #[test]
    fn every_point_lands_in_one_kilometre_and_neighbours_share_the_crossing() {
        let points: Vec<(f32, f32)> = (0..7).map(|i| (i as f32, 0.0)).collect();
        let distances = [0.0, 400.0, 800.0, 1200.0, 1600.0, 2000.0, 2300.0];
        assert_eq!(
            per_km(&points, &distances, 3),
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
        assert_eq!(
            per_km(&points, &[0.0, 2500.0], 3),
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
    fn without_kilometres_there_is_nothing_to_cut() {
        assert!(per_km(&[(0.0, 0.0), (1.0, 1.0)], &[0.0, 10.0], 0).is_empty());
        assert!(per_km(&[], &[], 3).is_empty());
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
            assert!(
                steps.iter().all(|&s| s > 0 && s <= steps[0]),
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
        let trail = trail_of(&analysis);
        assert_eq!(trail.segments.len(), views.len());
        for (segment, view) in trail.segments.iter().zip(&views) {
            assert_eq!((segment.index, &segment.color), (view.index, &view.color));
            assert!(segment.path.starts_with("M "), "{}", segment.path);
        }
    }

    #[test]
    fn neighbouring_segments_share_their_boundary_point() {
        let trail = trail_of(&analysis(track(25, 9)));
        for pair in trail.segments.windows(2) {
            let (before, after) = (path_points(&pair[0].path), path_points(&pair[1].path));
            assert!(before.len() >= 2 && after.len() >= 2, "{pair:?}");
            assert_eq!(before.last(), after.first(), "{pair:?}");
        }
    }

    #[test]
    fn the_boundaries_are_where_the_kilometres_end() {
        // 100 m steps east: km 1 ends 1000 m east of the start, a tenth of the
        // 2.5 km line, km 2 at 2000 m.
        let trail = trail_of(&analysis(track(25, 0)));
        assert_eq!(trail.segments.len(), 3);
        let km1 = path_points(&trail.segments[0].path);
        let km2 = path_points(&trail.segments[1].path);
        let x_at = |metres: f32| 72.0 + 576.0 * metres / 2500.0;
        assert_near(km1.last().unwrap().0, x_at(1000.0), "end of km 1");
        assert_near(km2.last().unwrap().0, x_at(2000.0), "end of km 2");
    }

    #[test]
    fn the_route_is_fitted_into_the_zone_inset_by_24_px_and_centred() {
        // East-west: the full width at the middle height of the zone.
        let line = all_points(&trail_of(&analysis(track(25, 0))));
        let (x0, y0, x1, y1) = extent(&line);
        assert_near(x0, 72.0, "min x");
        assert_near(x1, 648.0, "max x");
        assert_near(y0, 440.0, "min y");
        assert_near(y1, 440.0, "max y");
        // A 2.5 by 2.5 km L: a square of the zone's height, centred across.
        let square = all_points(&trail_of(&analysis(track(25, 25))));
        let (x0, y0, x1, y1) = extent(&square);
        assert_near(x0, 104.0, "min x");
        assert_near(x1, 616.0, "max x");
        assert_near(y0, 184.0, "min y");
        assert_near(y1, 696.0, "max y");
    }

    #[test]
    fn north_is_up() {
        let trail = trail_of(&analysis(track(25, 9)));
        let (start, finish) = (trail.start.unwrap(), trail.finish.unwrap());
        assert!(finish.1 < start.1, "{finish:?} is not above {start:?}");
        assert!(finish.0 > start.0, "{finish:?} is not east of {start:?}");
    }

    #[test]
    fn the_markers_are_the_first_and_last_fitted_point() {
        let analysis = analysis(track(25, 9));
        let fitted = route::fit(
            &route::project(&analysis.points),
            route::BBox {
                x: 72.0,
                y: 184.0,
                w: 576.0,
                h: 512.0,
            },
        );
        let trail = trail_of(&analysis);
        assert_eq!(trail.start, fitted.first().copied());
        assert_eq!(trail.finish, fitted.last().copied());
        let first = path_points(&trail.segments[0].path)[0];
        assert_near(first.0, trail.start.unwrap().0, "first path x");
        assert_near(first.1, trail.start.unwrap().1, "first path y");
    }

    #[test]
    fn repeated_points_are_dropped() {
        let mut points = track(25, 9);
        let stops: Vec<TrackPoint> = points
            .iter()
            .flat_map(|p| [p.clone(), p.clone(), p.clone()])
            .collect();
        points.clone_from(&stops);
        let trail = trail_of(&analysis(points));
        for segment in &trail.segments {
            let points = path_points(&segment.path);
            assert!(
                points.windows(2).all(|w| w[0] != w[1]),
                "km {} repeats a point: {}",
                segment.index,
                segment.path
            );
        }
        assert_eq!(trail.segments.len(), 4);
    }

    #[test]
    fn a_long_track_is_drawn_with_at_most_4000_points() {
        // 12 000 steps of 2 m east, then 2 000 north.
        let east = (0..=12_000).map(|i| point(f64::from(i) * 2.0, 0.0));
        let north = (1..=2_000).map(|i| point(24_000.0, f64::from(i) * 2.0));
        let analysis = analysis(east.chain(north).collect());
        let trail = trail_of(&analysis);
        assert_eq!(trail.segments.len(), analysis.splits.len());
        // Each boundary point is shared by two segments.
        let drawn = all_points(&trail).len() - (trail.segments.len() - 1);
        assert!(drawn <= 4000 + trail.segments.len(), "{drawn} points");
        let (x1, y0) = trail.finish.unwrap();
        let last = *path_points(&trail.segments.last().unwrap().path)
            .last()
            .unwrap();
        assert_near(last.0, x1, "last x");
        assert_near(last.1, y0, "last y");
    }

    #[test]
    fn fewer_than_two_distinct_points_leave_the_zone_empty() {
        let here = point(0.0, 0.0);
        let degenerate = [
            vec![],
            vec![here.clone()],
            vec![here.clone(), here.clone(), here],
        ];
        for points in degenerate {
            let analysis = Analysis {
                distance_m: 1500.0,
                elapsed_s: 450.0,
                splits: vec![
                    Split {
                        index: 1,
                        distance_m: 1000.0,
                        seconds: 300.0,
                        partial: false,
                    },
                    Split {
                        index: 2,
                        distance_m: 500.0,
                        seconds: 150.0,
                        partial: true,
                    },
                ],
                fastest: Some(1),
                start: None,
                points: points.clone(),
            };
            let trail = trail_of(&analysis);
            assert!(trail.segments.is_empty(), "{} points", points.len());
            assert_eq!((trail.start, trail.finish), (None, None));
            let value = trail.to_value();
            assert!(value.get_attr("start").unwrap().is_none());
            assert!(value.get_attr("finish").unwrap().is_none());
        }
    }

    #[test]
    fn the_template_value_has_every_number_rounded_to_two_decimals() {
        let trail = Trail {
            segments: vec![Segment {
                index: 2,
                path: "M 1.0 2.0 L 3.0 4.0".into(),
                color: "#123456".into(),
            }],
            start: Some((100.123_45, 200.987_65)),
            finish: Some((300.0, 400.0)),
        };
        let value = trail.to_value();
        let get = |path: &str| {
            path.split('.')
                .try_fold(value.clone(), |v, key| v.get_attr(key).ok())
                .unwrap_or_else(|| panic!("no {path}"))
                .to_string()
        };
        assert_eq!(get("start.x"), "100.12");
        assert_eq!(get("start.y"), "200.99");
        // The finish square is 7 px wide, centred on the last point.
        assert_eq!(get("finish.x"), "296.5");
        assert_eq!(get("finish.y"), "396.5");
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
        assert_eq!(segment.get_attr("index").unwrap().to_string(), "2");
    }
}
