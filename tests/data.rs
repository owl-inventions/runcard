//! The data layer end to end: synthetic GPX on disk, parsed, analysed,
//! formatted; and the bundled sample with the generator that writes it.

mod common;

use common::{Course, Shape};
use runcard::pipeline::{CardRequest, Pipeline};
use runcard::{format, geo, gpx, splits, Theme, Variant};
use time::macros::datetime;
use time::OffsetDateTime;

const START: OffsetDateTime = datetime!(2026-09-27 09:00:00 UTC);
const MARATHON_M: f64 = 42_195.0;

/// 5:30 per kilometre, except a 4:30 kilometre 30.
fn marathon_pace(km: u32) -> f64 {
    if km == 30 {
        270.0
    } else {
        330.0
    }
}

#[test]
fn a_synthetic_marathon_is_loaded_analysed_and_formatted() {
    let xml = common::synthetic_gpx(MARATHON_M, marathon_pace, START);
    let path = common::write_fixture("marathon.gpx", &xml);

    let track = gpx::load(&path).unwrap();
    assert_eq!(track.name.as_deref(), Some(common::TRACK_NAME));
    let analysis = splits::analyze(&track).unwrap();

    assert_eq!(analysis.splits.len(), 43);
    assert_eq!(analysis.fastest, Some(30));
    assert!(
        (analysis.distance_m - MARATHON_M).abs() < 10.0,
        "distance was {}",
        analysis.distance_m
    );
    let expected_s: f64 = (1..=42).map(marathon_pace).sum::<f64>() + 0.195 * marathon_pace(43);
    assert!(
        (analysis.elapsed_s - expected_s).abs() < 2.0,
        "elapsed {} expected {expected_s}",
        analysis.elapsed_s
    );
    assert_eq!(analysis.start, Some(START));
    assert_eq!(format::distance(analysis.distance_m), "42.20 km");
}

#[test]
fn the_synthetic_route_is_a_closed_loop_heading_east_first() {
    let xml = common::synthetic_gpx(10_000.0, |_| 300.0, START);
    let track = gpx::parse(xml.as_bytes()).unwrap();
    let points = &track.points;
    let (first, last) = (&points[0], &points[points.len() - 1]);

    assert_eq!((first.lat, first.lon), (52.5, 13.4));
    assert!(geo::haversine_m(first, last) < 1.0, "loop does not close");
    assert!(
        points[1].lon > first.lon && points[1].lat == first.lat,
        "not heading east"
    );
    let max_lat = points.iter().map(|p| p.lat).fold(f64::MIN, f64::max);
    assert!(max_lat > 52.51, "loop never goes north");
    let gaps: Vec<f64> = points
        .windows(2)
        .map(|w| geo::haversine_m(&w[0], &w[1]))
        .collect();
    assert!(
        gaps.iter().all(|g| (g - 50.0).abs() < 0.5),
        "points are not 50 m apart"
    );
}

#[test]
fn a_waypoint_course_starts_at_its_origin_and_closes_at_its_length() {
    let course = Course {
        name: "Square",
        start: (38.7, -9.1),
        step_m: 100.0,
        shape: Shape::Waypoints(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]),
    };
    let xml = common::course_gpx(&course, 4_000.0, |_| 300.0, START);
    let track = gpx::parse(xml.as_bytes()).unwrap();
    let points = &track.points;
    assert_eq!(track.name.as_deref(), Some("Square"));
    assert_eq!((points[0].lat, points[0].lon), (38.7, -9.1));
    assert_eq!(points.len(), 41);
    assert!(
        geo::haversine_m(&points[0], &points[40]) < 1.0,
        "the course does not close"
    );
    // A quarter of the way round is the first corner, 1 km east.
    assert!(geo::haversine_m(&points[0], &points[10]) > 990.0);
    assert!((points[10].lat - 38.7).abs() < 1e-6, "{:?}", points[10]);
    let length = geo::cumulative_m(points)[40];
    assert!((length - 4_000.0).abs() < 1.0, "{length} m");
}

#[test]
fn a_pace_list_times_each_kilometre_and_repeats_its_last_entry() {
    let pace = common::paces(&[300.0, 310.0, 290.0]);
    assert_eq!(
        [1, 2, 3, 4, 9].map(&pace),
        [300.0, 310.0, 290.0, 290.0, 290.0]
    );
}

/// `examples/<name>` in the crate.
fn example(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
}

fn sample_path() -> std::path::PathBuf {
    example("sample.gpx")
}

/// 08:30 in Lisbon on Sunday, 19 April 2026.
const SAMPLE_START: OffsetDateTime = datetime!(2026-04-19 07:30:00 UTC);
const SAMPLE_M: f64 = 21_100.0;

/// Seconds per kilometre for the bundled sample: an even 5:00 to 5:06 after
/// a crowded first kilometre, a climb from km 13 to 15 (5:52 at its top), a
/// fast finish down to 4:48 and the last 100 m at 4:50.
const SAMPLE_PACES: [f64; 22] = [
    318.0, 309.0, 302.0, 304.0, 300.0, 303.0, 301.0, 305.0, 302.0, 304.0, //
    306.0, 314.0, 338.0, 352.0, 345.0, 304.0, 306.0, 309.0, 304.0, 295.0, //
    288.0, 290.0,
];

/// The sample course: west along the river, up the hill and back east along
/// the ridge, in kilometres east and north of the start.
const HARBOUR: Course = Course {
    name: "Harbour Half",
    start: (38.7075, -9.1365),
    step_m: 100.0,
    shape: Shape::Waypoints(&[
        (0.0, 0.0),
        (-1.4, -0.3),
        (-2.9, -0.4),
        (-4.2, -0.9),
        (-5.6, -1.0),
        (-6.9, -1.5),
        (-7.6, -1.2),
        (-7.7, -0.4),
        (-7.0, 0.3),
        (-6.0, 0.6),
        (-5.2, 1.5),
        (-4.0, 2.1),
        (-2.6, 2.3),
        (-1.6, 1.7),
        (-0.7, 1.2),
        (-0.4, 0.5),
    ]),
};

#[test]
fn the_bundled_sample_is_a_1_48_58_half_marathon_in_lisbon_with_a_fast_finish() {
    let track = gpx::load(&sample_path()).unwrap();
    let analysis = splits::analyze(&track).unwrap();

    assert_eq!(track.name.as_deref(), Some("Harbour Half"));
    assert_eq!(track.points.len(), 212);
    assert_eq!(
        (track.points[0].lat, track.points[0].lon),
        (38.7075, -9.1365)
    );
    assert_eq!(analysis.start, Some(SAMPLE_START));
    assert_eq!(format::distance(analysis.distance_m), "21.10 km");
    assert_eq!(format::duration(analysis.elapsed_s), "1:48:58");
    assert_eq!(analysis.splits.len(), 22);
    assert!(analysis.splits[21].partial);
    assert_eq!(analysis.fastest, Some(21));

    let full = &analysis.splits[..21];
    let pace_of = |km: usize| format::pace(full[km - 1].pace_s_per_km());
    assert_eq!(pace_of(21), "4:48 /km");
    let slowest = full
        .iter()
        .max_by(|a, b| a.seconds.total_cmp(&b.seconds))
        .unwrap();
    assert_eq!(slowest.index, 14);
    assert_eq!(pace_of(14), "5:52 /km");
    let average = analysis.elapsed_s / analysis.distance_m * 1000.0;
    assert_eq!(format::pace(average), "5:10 /km");
}

/// Regenerates `examples/sample.gpx` and the cards rendered from it, as the
/// CLI renders them for `--runner "Mara Lindqvist" --city Lisbon`:
/// `examples/<variant>.png` for every variant at scale 1,
/// `examples/ribbon-dark.png` and `examples/sample.png`, the ribbon card at
/// scale 2. Run with `cargo test --test data -- --ignored`.
#[test]
#[ignore = "writes examples/sample.gpx and the example PNGs; run after changing the sample or a template"]
fn generate_samples() {
    let xml = common::course_gpx(
        &HARBOUR,
        SAMPLE_M,
        common::paces(&SAMPLE_PACES),
        SAMPLE_START,
    );
    std::fs::write(sample_path(), &xml).unwrap();

    let pipeline = Pipeline::new().unwrap();
    let card = |variant: Variant, theme: Theme, scale: f32| {
        let request = CardRequest {
            runner: "Mara Lindqvist".into(),
            event: None,
            city: Some("Lisbon".into()),
            date: None,
            scale,
            variant,
            theme,
        };
        pipeline.png(xml.as_bytes(), &request).unwrap()
    };
    for variant in Variant::ALL {
        let png = card(variant, Theme::Light, 1.0);
        std::fs::write(example(&format!("{variant}.png")), png).unwrap();
    }
    let dark = card(Variant::Ribbon, Theme::Dark, 1.0);
    std::fs::write(example("ribbon-dark.png"), dark).unwrap();
    let sample = card(Variant::Ribbon, Theme::Light, 2.0);
    std::fs::write(example("sample.png"), sample).unwrap();
}
