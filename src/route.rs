//! Projects the track coordinates into the route outline box of the card.

use crate::model::TrackPoint;

/// Bisection steps when searching the simplification tolerance.
const BISECTION_STEPS: usize = 40;

/// The box on the card the route outline is fitted into, in card pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The route as a polyline in card pixels: projected equirectangularly (north
/// up), fitted into `bbox` with its aspect ratio kept and centred, and
/// simplified with Ramer-Douglas-Peucker to at most `max_points` points.
pub fn outline(points: &[TrackPoint], bbox: BBox, max_points: usize) -> Vec<(f32, f32)> {
    let fitted = fit(&project(points), bbox);
    if fitted.len() <= max_points {
        return fitted;
    }
    let (mut low, mut high) = (0.0_f32, bbox.w.hypot(bbox.h));
    for _ in 0..BISECTION_STEPS {
        let mid = (low + high) / 2.0;
        if simplify_rdp(&fitted, mid).len() <= max_points {
            high = mid;
        } else {
            low = mid;
        }
    }
    let mut simplified = simplify_rdp(&fitted, high);
    simplified.truncate(max_points);
    simplified
}

/// Equirectangular projection around the mean latitude, y growing southwards.
/// Longitudes are taken within 180 degrees of the first point's, so a route
/// across the antimeridian stays in one piece.
pub(crate) fn project(points: &[TrackPoint]) -> Vec<(f64, f64)> {
    let Some(first) = points.first() else {
        return Vec::new();
    };
    let mean_lat = points.iter().map(|p| p.lat).sum::<f64>() / points.len() as f64;
    let k = mean_lat.to_radians().cos();
    let unwrap = |lon: f64| first.lon + (lon - first.lon + 180.0).rem_euclid(360.0) - 180.0;
    points.iter().map(|p| (unwrap(p.lon) * k, -p.lat)).collect()
}

/// Scales and translates projected points into `bbox`, keeping the aspect ratio
/// and centring the shorter side.
pub(crate) fn fit(points: &[(f64, f64)], bbox: BBox) -> Vec<(f32, f32)> {
    let Some(&(x, y)) = points.first() else {
        return Vec::new();
    };
    let (min_x, min_y, max_x, max_y) = points
        .iter()
        .fold((x, y, x, y), |(x0, y0, x1, y1), &(x, y)| {
            (x0.min(x), y0.min(y), x1.max(x), y1.max(y))
        });
    let (span_x, span_y) = (max_x - min_x, max_y - min_y);
    let (w, h) = (f64::from(bbox.w), f64::from(bbox.h));
    let scale = match (span_x > 0.0, span_y > 0.0) {
        (true, true) => (w / span_x).min(h / span_y),
        (true, false) => w / span_x,
        (false, true) => h / span_y,
        (false, false) => 0.0,
    };
    let left = f64::from(bbox.x) + (w - span_x * scale) / 2.0;
    let top = f64::from(bbox.y) + (h - span_y * scale) / 2.0;
    points
        .iter()
        .map(|&(x, y)| {
            (
                (left + (x - min_x) * scale) as f32,
                (top + (y - min_y) * scale) as f32,
            )
        })
        .collect()
}

/// Ramer-Douglas-Peucker: drops every point closer than `epsilon` to the line
/// that replaces it. The first and the last point are always kept.
pub fn simplify_rdp(points: &[(f32, f32)], epsilon: f32) -> Vec<(f32, f32)> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut ranges = vec![(0, points.len() - 1)];
    while let Some((first, last)) = ranges.pop() {
        let farthest = (first + 1..last)
            .map(|i| (i, segment_distance(points[i], points[first], points[last])))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, distance)) = farthest {
            if distance > epsilon {
                keep[i] = true;
                ranges.push((first, i));
                ranges.push((i, last));
            }
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(&p, kept)| kept.then_some(p))
        .collect()
}

/// Distance from `p` to the segment `a`-`b` (to `a` when the segment is a point,
/// as for the closing point of a loop).
fn segment_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length_sq = dx * dx + dy * dy;
    let t = if length_sq > 0.0 {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / length_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.0 - (a.0 + t * dx)).hypot(p.1 - (a.1 + t * dy))
}

/// SVG path data for the polyline, one decimal: `M x y L x y ...`.
pub fn to_path(points: &[(f32, f32)]) -> String {
    points
        .iter()
        .enumerate()
        .map(|(i, (x, y))| format!("{} {x:.1} {y:.1}", if i == 0 { "M" } else { "L" }))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: BBox = BBox {
        x: 520.0,
        y: 620.0,
        w: 160.0,
        h: 100.0,
    };

    fn point(lat: f64, lon: f64) -> TrackPoint {
        TrackPoint {
            lat,
            lon,
            time: None,
        }
    }

    /// A closed square of about 1 km side south-west anchored at 52.5 N 13.4 E,
    /// walked with `per_side` points on every side.
    fn square_loop(per_side: usize) -> Vec<TrackPoint> {
        let dlat = 1.0 / 111.32;
        let dlon = dlat / 52.5_f64.to_radians().cos();
        let corners = [
            (52.5, 13.4),
            (52.5, 13.4 + dlon),
            (52.5 + dlat, 13.4 + dlon),
            (52.5 + dlat, 13.4),
        ];
        let mut points = Vec::new();
        for side in 0..4 {
            let (lat0, lon0) = corners[side];
            let (lat1, lon1) = corners[(side + 1) % 4];
            for i in 0..per_side {
                let f = i as f64 / per_side as f64;
                points.push(point(lat0 + (lat1 - lat0) * f, lon0 + (lon1 - lon0) * f));
            }
        }
        points.push(points[0].clone());
        points
    }

    fn extent(points: &[(f32, f32)]) -> (f32, f32, f32, f32) {
        points.iter().fold(
            (f32::MAX, f32::MAX, f32::MIN, f32::MIN),
            |(x0, y0, x1, y1), &(x, y)| (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
        )
    }

    fn assert_near(actual: f32, expected: f32, what: &str) {
        assert!(
            (actual - expected).abs() <= 0.5,
            "{what} was {actual}, expected {expected}"
        );
    }

    #[test]
    fn a_square_loop_fills_the_box_as_a_centred_square() {
        let outline = outline(&square_loop(10), BOX, 500);
        let (min_x, min_y, max_x, max_y) = extent(&outline);
        assert_near(min_x, 550.0, "min x");
        assert_near(max_x, 650.0, "max x");
        assert_near(min_y, 620.0, "min y");
        assert_near(max_y, 720.0, "max y");
    }

    #[test]
    fn a_loop_across_the_antimeridian_stays_a_compact_square() {
        // A 0.002 degree square on the equator from 179.999 E to 179.999 W.
        let wrap = |lon: f64| if lon > 180.0 { lon - 360.0 } else { lon };
        let corners = [
            (0.0, 179.999),
            (0.0, 180.001),
            (0.002, 180.001),
            (0.002, 179.999),
            (0.0, 179.999),
        ];
        let points: Vec<TrackPoint> = corners
            .iter()
            .map(|&(lat, lon)| point(lat, wrap(lon)))
            .collect();
        assert_eq!(points[1].lon, -179.999);
        let outline = outline(&points, BOX, 500);
        let (min_x, min_y, max_x, max_y) = extent(&outline);
        assert_near(min_x, 550.0, "min x");
        assert_near(max_x, 650.0, "max x");
        assert_near(min_y, 620.0, "min y");
        assert_near(max_y, 720.0, "max y");
    }

    #[test]
    fn a_straight_east_west_line_spans_the_box_width_at_mid_height() {
        let line = [point(52.5, 13.40), point(52.5, 13.41), point(52.5, 13.42)];
        let outline = outline(&line, BOX, 500);
        let (min_x, min_y, max_x, max_y) = extent(&outline);
        assert_near(min_x, 520.0, "min x");
        assert_near(max_x, 680.0, "max x");
        assert_near(min_y, 670.0, "min y");
        assert_near(max_y, 670.0, "max y");
        assert_near(outline[1].0, 600.0, "middle point x");
    }

    #[test]
    fn a_straight_north_south_line_spans_the_box_height_at_mid_width() {
        let line = [point(52.50, 13.4), point(52.51, 13.4), point(52.52, 13.4)];
        let outline = outline(&line, BOX, 500);
        let (min_x, min_y, max_x, max_y) = extent(&outline);
        assert_near(min_x, 600.0, "min x");
        assert_near(max_x, 600.0, "max x");
        assert_near(min_y, 620.0, "min y");
        assert_near(max_y, 720.0, "max y");
        assert_near(outline[1].1, 670.0, "middle point y");
    }

    #[test]
    fn north_is_up() {
        let outline = outline(&square_loop(10), BOX, 500);
        let start = outline[0];
        let north_east = outline[20];
        assert!(
            north_east.1 < start.1,
            "{north_east:?} is not above {start:?}"
        );
    }

    #[test]
    fn an_empty_track_has_no_outline() {
        assert!(outline(&[], BOX, 100).is_empty());
    }

    #[test]
    fn rdp_drops_collinear_midpoints_and_keeps_the_corners() {
        let square = [
            (0.0, 0.0),
            (5.0, 0.0),
            (10.0, 0.0),
            (10.0, 5.0),
            (10.0, 10.0),
            (5.0, 10.0),
            (0.0, 10.0),
            (0.0, 5.0),
            (0.0, 0.0),
        ];
        assert_eq!(
            simplify_rdp(&square, 0.1),
            vec![
                (0.0, 0.0),
                (10.0, 0.0),
                (10.0, 10.0),
                (0.0, 10.0),
                (0.0, 0.0)
            ]
        );
    }

    #[test]
    fn the_outline_respects_the_point_budget() {
        let circle: Vec<TrackPoint> = (0..1000)
            .map(|i| {
                let a = i as f64 / 1000.0 * std::f64::consts::TAU;
                point(52.5 + 0.01 * a.sin(), 13.4 + 0.016 * a.cos())
            })
            .collect();
        let outline = outline(&circle, BOX, 60);
        assert!(outline.len() <= 60, "{} points", outline.len());
        assert!(
            outline.len() >= 30,
            "over-simplified to {} points",
            outline.len()
        );
    }

    #[test]
    fn a_short_track_is_kept_whole() {
        let outline = outline(&square_loop(3), BOX, 500);
        assert_eq!(outline.len(), 13);
    }

    #[test]
    fn the_path_moves_to_the_first_point_and_draws_lines_to_the_rest() {
        let path = to_path(&[(540.0, 630.0), (541.23, 631.67), (542.0, 633.0)]);
        assert_eq!(path, "M 540.0 630.0 L 541.2 631.7 L 542.0 633.0");
    }

    #[test]
    fn an_empty_outline_is_an_empty_path() {
        assert_eq!(to_path(&[]), "");
    }
}
