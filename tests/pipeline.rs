//! The shared pipeline: GPX bytes and a request in, the card as SVG and PNG out.

mod common;

use std::path::Path;

use runcard::pipeline::{CardRequest, Pipeline};
use runcard::{Error, Variant};
use time::macros::{date, datetime};
use time::OffsetDateTime;

const START: OffsetDateTime = datetime!(2026-09-27 09:00:00 UTC);

fn ten_k() -> String {
    common::synthetic_gpx(10_000.0, |km| if km == 7 { 270.0 } else { 300.0 }, START)
}

fn request() -> CardRequest {
    CardRequest {
        runner: "Julia Brandt".into(),
        event: None,
        city: None,
        date: None,
        scale: 2.0,
        variant: Variant::default(),
    }
}

/// Width and height of a PNG.
fn png_size(png_bytes: &[u8]) -> (u32, u32) {
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let info = decoder.read_info().expect("a valid png header");
    (info.info().width, info.info().height)
}

#[test]
fn the_pipeline_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Pipeline>();
}

#[test]
fn the_png_has_the_card_size_times_the_scale() {
    let pipeline = Pipeline::new().unwrap();
    let xml = ten_k();
    let png = pipeline.png(xml.as_bytes(), &request()).unwrap();
    assert_eq!(png_size(&png), (1440, 2000));
    let small = CardRequest {
        scale: 1.0,
        ..request()
    };
    assert_eq!(
        png_size(&pipeline.png(xml.as_bytes(), &small).unwrap()),
        (720, 1000)
    );
}

#[test]
fn the_svg_shows_the_runner_event_city_and_date_of_the_request() {
    let req = CardRequest {
        event: Some("Spreelauf".into()),
        city: Some("Berlin".into()),
        date: Some(date!(2026 - 05 - 03)),
        ..request()
    };
    let svg = Pipeline::new()
        .unwrap()
        .svg(ten_k().as_bytes(), &req)
        .unwrap();
    assert!(svg.contains(">JULIA BRANDT</text>"), "{svg}");
    assert!(svg.contains(">Spreelauf</text>"), "{svg}");
    assert!(svg.contains("BERLIN"), "{svg}");
    assert!(svg.contains("MAY 3, 2026"), "{svg}");
}

#[test]
fn without_event_and_date_the_card_takes_the_track_name_and_start_date() {
    let svg = Pipeline::new()
        .unwrap()
        .svg(ten_k().as_bytes(), &request())
        .unwrap();
    assert!(svg.contains(">Berlin Marathon</text>"), "{svg}");
    assert!(svg.contains("SEPTEMBER 27, 2026"), "{svg}");
}

#[test]
fn render_returns_both_formats_and_the_time_of_each_stage() {
    let rendered = Pipeline::new()
        .unwrap()
        .render(ten_k().as_bytes(), &request())
        .unwrap();
    assert!(rendered.svg.contains(">JULIA BRANDT</text>"));
    assert_eq!(png_size(&rendered.png), (1440, 2000));
    let stages: Vec<&str> = rendered.timings.iter().map(|(stage, _)| *stage).collect();
    assert_eq!(stages, ["parse", "analyze", "layout", "svg", "png"]);
}

#[test]
fn a_track_without_timestamps_is_the_analysis_error() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="runcard-tests" xmlns="http://www.topografix.com/GPX/1/1">
<trk><trkseg><trkpt lat="52.5" lon="13.4"/><trkpt lat="52.51" lon="13.41"/></trkseg></trk>
</gpx>"#;
    let error = Pipeline::new()
        .unwrap()
        .png(xml.as_bytes(), &request())
        .unwrap_err();
    assert!(matches!(error, Error::NoTimestamps), "{error:?}");
}

#[test]
fn malformed_xml_is_the_gpx_error() {
    let error = Pipeline::new()
        .unwrap()
        .svg("<gpx version=\"1.1\"><trk>".as_bytes(), &request())
        .unwrap_err();
    assert!(matches!(error, Error::Gpx(_)), "{error:?}");
}

/// Width, height and RGB bytes of a PNG.
fn decode(png_bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = decoder.read_info().expect("a valid png header");
    let mut buf = vec![0; reader.output_buffer_size().expect("a sane png size")];
    let info = reader.next_frame(&mut buf).expect("a valid png frame");
    assert_eq!(info.color_type, png::ColorType::Rgb);
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

fn example(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
}

#[test]
fn the_default_request_renders_the_classic_sample_card_unchanged() {
    let gpx = std::fs::read(example("sample.gpx")).unwrap();
    let req = CardRequest {
        city: Some("Berlin".into()),
        ..request()
    };
    let pipeline = Pipeline::new().unwrap();
    let classic = CardRequest {
        variant: Variant::Classic,
        ..req.clone()
    };
    assert_eq!(
        pipeline.svg(&gpx[..], &req).unwrap(),
        pipeline.svg(&gpx[..], &classic).unwrap()
    );

    let (width, height, actual) = decode(&pipeline.png(&gpx[..], &req).unwrap());
    let (_, _, expected) = decode(&std::fs::read(example("sample.png")).unwrap());
    assert_eq!((width, height), (1440, 2000));
    let worst = actual
        .iter()
        .zip(&expected)
        .map(|(a, e)| a.abs_diff(*e))
        .max();
    assert!(
        actual.len() == expected.len() && worst <= Some(2),
        "the card differs from examples/sample.png by up to {worst:?} per channel"
    );
}

#[test]
fn noir_takes_the_dark_theme_and_the_dark_palette() {
    let pipeline = Pipeline::new().unwrap();
    let noir = CardRequest {
        variant: Variant::Noir,
        ..request()
    };
    let svg = pipeline.svg(ten_k().as_bytes(), &noir).unwrap();
    assert!(svg.contains("feGaussianBlur"), "{svg}");
    assert!(svg.contains(r##"fill="#0f1115""##), "{svg}");
    // The slowest swatch of the legend: violet, not the light scale's slate.
    assert!(svg.contains(r##"fill="#3b2a6b""##), "{svg}");
    assert!(!svg.contains("#2c4a5e"), "{svg}");

    let classic = pipeline.svg(ten_k().as_bytes(), &request()).unwrap();
    assert!(classic.contains(r##"fill="#2c4a5e""##), "{classic}");
    assert!(!classic.contains("#3b2a6b"), "{classic}");
}

#[test]
fn every_variant_renders_a_card_of_the_requested_size() {
    let pipeline = Pipeline::new().unwrap();
    for variant in Variant::ALL {
        let req = CardRequest {
            variant,
            scale: 1.0,
            ..request()
        };
        let rendered = pipeline.render(ten_k().as_bytes(), &req).unwrap();
        assert!(rendered.svg.contains(">JULIA BRANDT</text>"), "{variant}");
        assert_eq!(png_size(&rendered.png), (720, 1000), "{variant}");
    }
}
