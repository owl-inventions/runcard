//! The shared pipeline: GPX bytes and a request in, the card as SVG and PNG out.

mod common;

use runcard::pipeline::{CardRequest, Pipeline};
use runcard::Error;
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
