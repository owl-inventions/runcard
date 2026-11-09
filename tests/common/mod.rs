//! Synthetic GPX fixtures shared by the integration tests.
#![allow(dead_code)]

use std::f64::consts::{FRAC_PI_2, PI};
use std::fmt::Write as _;
use std::path::PathBuf;

use runcard::geo::{self, EARTH_RADIUS_M};
use runcard::TrackPoint;
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

pub const START_LAT: f64 = 52.5;
pub const START_LON: f64 = 13.4;
pub const TRACK_NAME: &str = "Harbour Half";

/// Where and how a synthetic track runs.
#[derive(Debug, Clone, Copy)]
pub struct Course {
    /// The track's `<name>`.
    pub name: &'static str,
    /// Latitude and longitude of the first point.
    pub start: (f64, f64),
    /// Metres between neighbouring points.
    pub step_m: f64,
    pub shape: Shape,
}

/// The outline of a closed course, scaled to the track's length.
#[derive(Debug, Clone, Copy)]
pub enum Shape {
    /// A counter-clockwise rounded rectangle, heading east first.
    RoundedRectangle,
    /// Straight legs through these (east, north) waypoints and back to the
    /// first one, in any unit.
    Waypoints(&'static [(f64, f64)]),
}

impl Course {
    /// "Harbour Half", a rounded-rectangle loop from 38.7 N, 9.1 W with a
    /// point every `step_m` metres.
    pub fn loop_every(step_m: f64) -> Self {
        Self {
            name: TRACK_NAME,
            start: (START_LAT, START_LON),
            step_m,
            shape: Shape::RoundedRectangle,
        }
    }
}

/// A GPX 1.1 document with one track, "Harbour Half": a rounded-rectangle loop
/// of `total_m` metres starting at 38.7 N, 9.1 W, with a point every 50 m timed
/// by `pace_for_km` (seconds per kilometre, for 1-based kilometre numbers).
pub fn synthetic_gpx(
    total_m: f64,
    pace_for_km: impl Fn(u32) -> f64,
    start: OffsetDateTime,
) -> String {
    synthetic_gpx_every(50.0, total_m, pace_for_km, start)
}

/// Like [`synthetic_gpx`], with a point every `step_m` metres.
pub fn synthetic_gpx_every(
    step_m: f64,
    total_m: f64,
    pace_for_km: impl Fn(u32) -> f64,
    start: OffsetDateTime,
) -> String {
    course_gpx(&Course::loop_every(step_m), total_m, pace_for_km, start)
}

/// The pace function of a per-kilometre list: kilometre k at `paces[k - 1]`
/// seconds per kilometre, the last entry for any kilometre beyond the list.
pub fn paces(paces: &'static [f64]) -> impl Fn(u32) -> f64 {
    move |km: u32| paces[(km as usize).clamp(1, paces.len()) - 1]
}

/// A GPX 1.1 document with one track along `course`, `total_m` metres long
/// and timed by `pace_for_km`, starting at `start`.
pub fn course_gpx(
    course: &Course,
    total_m: f64,
    pace_for_km: impl Fn(u32) -> f64,
    start: OffsetDateTime,
) -> String {
    let points = course_points(course, total_m);
    let distances = geo::cumulative_m(&points);
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <gpx version=\"1.1\" creator=\"runcard synthetic fixture\" xmlns=\"http://www.topografix.com/GPX/1/1\">\n\
         \x20 <trk>\n\
         \x20   <name>{}</name>\n\
         \x20   <type>running</type>\n\
         \x20   <trkseg>\n",
        course.name
    );
    for (point, distance) in points.iter().zip(distances) {
        let offset = Duration::seconds(elapsed_s(distance, &pace_for_km).round() as i64);
        let time = (start + offset).format(&Rfc3339).expect("format timestamp");
        writeln!(
            xml,
            "      <trkpt lat=\"{:.7}\" lon=\"{:.7}\"><time>{time}</time></trkpt>",
            point.lat, point.lon
        )
        .expect("write to string");
    }
    xml.push_str("    </trkseg>\n  </trk>\n</gpx>\n");
    xml
}

/// Seconds from the start until `distance_m`, kilometre k run at `pace_for_km(k)`.
pub fn elapsed_s(distance_m: f64, pace_for_km: &impl Fn(u32) -> f64) -> f64 {
    let whole_km = (distance_m / 1000.0).floor() as u32;
    let full: f64 = (1..=whole_km).map(pace_for_km).sum();
    let rest_m = distance_m - f64::from(whole_km) * 1000.0;
    full + rest_m / 1000.0 * pace_for_km(whole_km + 1)
}

/// This test process's own directory in the temp dir, created on first use,
/// so concurrent test runs do not overwrite each other's files.
pub fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("runcard-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the test temp dir");
    dir
}

/// Writes `xml` to `<temp dir>/runcard-tests-<pid>/<name>` and returns the path.
pub fn write_fixture(name: &str, xml: &str) -> PathBuf {
    let path = temp_dir().join(name);
    std::fs::write(&path, xml).expect("write fixture");
    path
}

/// Points every `course.step_m` along the course whose measured (haversine)
/// length is `total_m`.
fn course_points(course: &Course, total_m: f64) -> Vec<TrackPoint> {
    // Chords cut the corners, so the points measure a little shorter than the
    // drawn course; a few rounds of rescaling make the measured length match.
    let mut perimeter = total_m;
    let mut points = sample_course(course, perimeter, total_m);
    for _ in 0..3 {
        let measured = geo::cumulative_m(&points)[points.len() - 1];
        perimeter *= total_m / measured;
        points = sample_course(course, perimeter, total_m);
    }
    points
}

fn sample_course(course: &Course, perimeter: f64, total_m: f64) -> Vec<TrackPoint> {
    let steps = (total_m / course.step_m).ceil() as u32;
    (0..=steps)
        .map(|i| {
            let fraction = (f64::from(i) * course.step_m / total_m).min(1.0);
            let s = fraction * perimeter;
            let (east, north) = match course.shape {
                Shape::RoundedRectangle => rounded_rectangle(s, perimeter),
                Shape::Waypoints(waypoints) => along_waypoints(waypoints, s, perimeter),
            };
            to_track_point(course.start, east, north)
        })
        .collect()
}

/// Metres east and north of the first waypoint after `s` metres along the
/// closed polygon through `waypoints`, scaled to `perimeter` metres.
fn along_waypoints(waypoints: &[(f64, f64)], s: f64, perimeter: f64) -> (f64, f64) {
    let legs: Vec<((f64, f64), (f64, f64))> = waypoints
        .iter()
        .zip(waypoints.iter().cycle().skip(1))
        .map(|(&a, &b)| (a, b))
        .collect();
    let length = |((ax, ay), (bx, by)): ((f64, f64), (f64, f64))| (bx - ax).hypot(by - ay);
    let scale = perimeter / legs.iter().map(|&leg| length(leg)).sum::<f64>();
    let (x0, y0) = waypoints[0];
    let mut rest = s / scale;
    for &leg in &legs {
        let ((ax, ay), (bx, by)) = leg;
        let along = length(leg);
        if rest <= along {
            let f = rest / along;
            return (
                (ax + (bx - ax) * f - x0) * scale,
                (ay + (by - ay) * f - y0) * scale,
            );
        }
        rest -= along;
    }
    (0.0, 0.0)
}

/// Metres east and north of the start after `s` metres along a counter-clockwise
/// rounded rectangle (east, north, west, south): straight sides 2b and b, corner
/// radius b/3.
fn rounded_rectangle(s: f64, perimeter: f64) -> (f64, f64) {
    let b = perimeter / (6.0 + 2.0 * PI / 3.0);
    let radius = b / 3.0;
    let (mut east, mut north, mut heading, mut rest) = (0.0, 0.0, 0.0_f64, s);
    for straight in [2.0 * b, b, 2.0 * b, b] {
        let along = rest.clamp(0.0, straight);
        east += along * heading.cos();
        north += along * heading.sin();
        rest -= along;
        let turned = (rest / radius).clamp(0.0, FRAC_PI_2);
        let centre = (
            east - radius * heading.sin(),
            north + radius * heading.cos(),
        );
        east = centre.0 + radius * (heading + turned).sin();
        north = centre.1 - radius * (heading + turned).cos();
        rest -= turned * radius;
        heading += FRAC_PI_2;
    }
    (east, north)
}

/// Converts metres east and north of `start` to a point, rounded to the 7
/// decimals written to the GPX.
fn to_track_point((start_lat, start_lon): (f64, f64), east: f64, north: f64) -> TrackPoint {
    let metres_per_degree = EARTH_RADIUS_M.to_radians();
    let lat = start_lat + north / metres_per_degree;
    let lon = start_lon + east / (metres_per_degree * lat.to_radians().cos());
    TrackPoint {
        lat: round7(lat),
        lon: round7(lon),
        time: None,
    }
}

fn round7(degrees: f64) -> f64 {
    (degrees * 1e7).round() / 1e7
}
