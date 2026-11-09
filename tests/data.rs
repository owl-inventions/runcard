//! The data layer end to end: synthetic GPX on disk, parsed, analysed, formatted.

mod common;

use runcard::pipeline::{CardRequest, Pipeline};
use runcard::{format, geo, gpx, splits, Variant};
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
    assert_eq!(track.name.as_deref(), Some("Berlin Marathon"));
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
    assert_eq!(format::distance(analysis.distance_m), "42.195 km");
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

/// `examples/<name>` in the crate.
fn example(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
}

fn sample_path() -> std::path::PathBuf {
    example("sample.gpx")
}

#[test]
fn the_bundled_sample_is_a_3_58_41_marathon_with_km_30_fastest() {
    let track = gpx::load(&sample_path()).unwrap();
    let analysis = splits::analyze(&track).unwrap();

    assert_eq!(track.name.as_deref(), Some("Berlin Marathon"));
    assert!(
        (400..=430).contains(&track.points.len()),
        "{} points",
        track.points.len()
    );
    assert_eq!(analysis.start, Some(START));
    assert_eq!(format::distance(analysis.distance_m), "42.195 km");
    assert_eq!(format::duration(analysis.elapsed_s), "3:58:41");
    assert_eq!(analysis.splits.len(), 43);
    assert_eq!(analysis.fastest, Some(30));

    let full = &analysis.splits[..42];
    let km30 = &full[29];
    assert_eq!(format::pace(km30.pace_s_per_km()), "5:16 /km");
    let slowest = full
        .iter()
        .max_by(|a, b| a.seconds.total_cmp(&b.seconds))
        .unwrap();
    assert_eq!(slowest.index, 38);
    assert_eq!(format::pace(slowest.pace_s_per_km()), "6:05 /km");
    let average = analysis.elapsed_s / analysis.distance_m * 1000.0;
    assert_eq!(format::pace(average), "5:39 /km");
}

/// Seconds per kilometre for the bundled sample: a steady first half, a surge
/// at km 30 (5:16), a fade to 6:05 at km 38 and a 5:10 kick on the last 195 m.
const SAMPLE_PACES: [f64; 43] = [
    345.0, 337.0, 333.0, 331.0, 332.0, 330.0, 334.0, 331.0, 329.0, 332.0, //
    335.0, 331.0, 333.0, 330.0, 334.0, 336.0, 332.0, 335.0, 337.0, 334.0, //
    339.0, 336.0, 338.0, 341.0, 337.0, 340.0, 343.0, 339.0, 331.0, 316.0, //
    342.0, 345.0, 348.0, 351.0, 353.0, 357.0, 361.0, 365.0, 359.0, 355.0, //
    350.0, 344.0, 310.0,
];

/// Regenerates `examples/sample.gpx` and the cards rendered from it, as the
/// CLI renders them for `--runner "Julia Brandt" --city Berlin`:
/// `examples/<variant>.png` for every variant at scale 1 and
/// `examples/sample.png`, the classic card at scale 2. Run with
/// `cargo test --test data -- --ignored`.
#[test]
#[ignore = "writes examples/sample.gpx and the example PNGs; run after changing SAMPLE_PACES or a template"]
fn generate_samples() {
    let pace = |km: u32| SAMPLE_PACES[km as usize - 1];
    let xml = common::synthetic_gpx_every(100.0, MARATHON_M, pace, START);
    std::fs::write(sample_path(), &xml).unwrap();

    let pipeline = Pipeline::new().unwrap();
    let card = |variant: Variant, scale: f32| {
        let request = CardRequest {
            runner: "Julia Brandt".into(),
            event: None,
            city: Some("Berlin".into()),
            date: None,
            scale,
            variant,
        };
        pipeline.png(xml.as_bytes(), &request).unwrap()
    };
    for variant in Variant::ALL {
        std::fs::write(example(&format!("{variant}.png")), card(variant, 1.0)).unwrap();
    }
    std::fs::write(example("sample.png"), card(Variant::Classic, 2.0)).unwrap();
}
