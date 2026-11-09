//! The `bars` and `mosaic` variants end to end: GPX in, SVG and PNG out.

mod common;

use runcard::palette::Palette;
use runcard::pipeline::{CardRequest, Pipeline};
use runcard::render::Renderer;
use runcard::text::TextMeasure;
use runcard::{card, gpx, splits, Meta, Theme, Variant};
use time::macros::datetime;
use time::OffsetDateTime;

const START: OffsetDateTime = datetime!(2026-09-27 09:00:00 UTC);

/// 10.46 km at 5:00, km 7 in 4:30: ten full kilometres and a partial.
fn ten_k() -> String {
    common::synthetic_gpx(10_460.0, |km| if km == 7 { 270.0 } else { 300.0 }, START)
}

/// A 21.1 km run: 21 full kilometres and a partial, at paces from 4:40 to 6:00.
fn half() -> String {
    common::synthetic_gpx(21_100.0, |km| 280.0 + f64::from(km * 7 % 17) * 5.0, START)
}

fn request(variant: Variant, scale: f32) -> CardRequest {
    CardRequest {
        runner: "Julia Brandt".into(),
        event: None,
        city: Some("Berlin".into()),
        date: None,
        scale,
        variant,
    }
}

fn svg_of(xml: &str, variant: Variant) -> String {
    Pipeline::new()
        .unwrap()
        .svg(xml.as_bytes(), &request(variant, 1.0))
        .unwrap()
}

fn split_count(xml: &str) -> usize {
    splits::analyze(&gpx::parse(xml.as_bytes()).unwrap())
        .unwrap()
        .splits
        .len()
}

/// Width, height and RGB bytes of a PNG.
fn decode(png_bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = decoder.read_info().expect("a valid png header");
    let mut buf = vec![0; reader.output_buffer_size().expect("a sane png size")];
    let info = reader.next_frame(&mut buf).expect("a valid png frame");
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

fn pixel((width, _, rgb): &(u32, u32, Vec<u8>), x: u32, y: u32) -> [u8; 3] {
    let i = ((y * width + x) * 3) as usize;
    [rgb[i], rgb[i + 1], rgb[i + 2]]
}

fn rgb_of(hex: &str) -> [u8; 3] {
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
    [channel(1), channel(3), channel(5)]
}

/// The value of the numeric attribute `name` in the tag `tag`.
fn number(tag: &str, name: &str) -> f32 {
    let key = format!(r#" {name}=""#);
    let start = tag
        .find(&key)
        .unwrap_or_else(|| panic!("no {name} in {tag}"))
        + key.len();
    let value = &tag[start..start + tag[start..].find('"').unwrap()];
    value.parse().unwrap_or_else(|_| panic!("{name}={value:?}"))
}

/// The heights of the bars, the `<rect>`s with `rx="2"` and a colour fill.
fn bar_heights(svg: &str) -> Vec<f32> {
    svg.split("<rect")
        .filter(|tag| tag.contains(r##"rx="2" fill="#"##))
        .map(|tag| number(tag, "height"))
        .collect()
}

fn assert_rendered(svg: &str) {
    assert!(svg.starts_with("<svg"), "{svg}");
    assert!(!svg.contains("{{") && !svg.contains("{%"), "{svg}");
    assert!(svg.contains(">JULIA BRANDT</text>"), "{svg}");
    assert!(svg.contains(">RUN \u{b7} BERLIN</text>"), "{svg}");
}

#[test]
fn bars_draws_one_rounded_bar_per_kilometre_and_the_dashed_average_line() {
    let xml = ten_k();
    let svg = svg_of(&xml, Variant::Bars);
    assert_rendered(&svg);
    assert_eq!(split_count(&xml), 11);
    assert_eq!(svg.matches(r##"rx="2" fill="#"##).count(), 11, "{svg}");
    assert!(
        svg.contains(r#"stroke-width="1.5" stroke-dasharray="4 4""#),
        "{svg}"
    );
    assert!(
        svg.contains(r#"x1="32""#) && svg.contains(r#"x2="688""#),
        "{svg}"
    );
    assert!(svg.contains(">AVG 4:57</text>"), "{svg}");
    assert!(svg.contains(r#"text-anchor="end""#), "{svg}");
    // The line sits at the average's palette position, 80 px over the
    // baseline for the slow anchor to 370 px for the fast one.
    let analysis = splits::analyze(&gpx::parse(xml.as_bytes()).unwrap()).unwrap();
    let meta = Meta {
        runner: "Julia Brandt".into(),
        event: "Spreelauf".into(),
        city: None,
        date: time::macros::date!(2026 - 09 - 27),
    };
    let avg_t = card::build(
        &analysis,
        &meta,
        &Palette::light(),
        &TextMeasure::new(),
        Variant::Bars,
    )
    .avg_t;
    assert!((0.1..0.9).contains(&avg_t), "average position {avg_t}");
    let avg_y = 560.0 - (80.0 + 290.0 * avg_t);
    let line = svg
        .split("<line")
        .find(|tag| tag.contains("stroke-dasharray"))
        .expect("the average line");
    assert!((number(line, "y1") - avg_y).abs() < 0.01, "{line}");
    // The label sits on a paper plate 3 px larger than its 57.6 x 8.4 px,
    // its baseline 6 px above the line.
    let plate = svg
        .split("<rect")
        .find(|tag| tag.contains(r##"width="63.6" height="14.4" fill="#f4efe6""##))
        .expect("the label's plate");
    assert!((number(plate, "x") - 627.4).abs() < 0.01, "{plate}");
    assert!(
        (number(plate, "y") - (avg_y - 6.0 - 11.4)).abs() < 0.01,
        "{plate}"
    );
    assert!(svg.contains(">KM</text>"), "{svg}");
    assert!(
        svg.contains(">5</text>") && svg.contains(">10</text>"),
        "{svg}"
    );
    assert!(
        svg.contains(">FASTEST KM 7 \u{b7} 4:30 &#x2f;KM</text>"),
        "{svg}"
    );
    assert!(svg.contains(r#"cy="178.0" r="4""#), "{svg}");
}

#[test]
fn the_bars_png_has_the_card_size_and_the_fastest_bar_reaches_the_top() {
    let pipeline = Pipeline::new().unwrap();
    let xml = ten_k();
    let image = decode(
        &pipeline
            .png(xml.as_bytes(), &request(Variant::Bars, 1.0))
            .unwrap(),
    );
    assert_eq!((image.0, image.1), (720, 1000));
    let large = pipeline
        .png(xml.as_bytes(), &request(Variant::Bars, 2.0))
        .unwrap();
    assert_eq!((decode(&large).0, decode(&large).1), (1440, 2000));

    // Km 7 is the seventh of 10.46 stripe widths, each (656 - 20) / 11 px.
    let width = (656.0 - 20.0) / 11.0;
    let km7 = (32.0 + 6.0 * (width + 2.0) + width / 2.0) as u32;
    let fastest = rgb_of(&Palette::light().hex(1.0));
    assert_eq!(pixel(&image, km7, 194), fastest, "top of km 7");
    assert_eq!(pixel(&image, km7, 555), fastest, "bottom of km 7");
    assert_eq!(
        pixel(&image, km7, 565),
        rgb_of("#f4efe6"),
        "under the baseline"
    );
    let km1 = (32.0 + width / 2.0) as u32;
    assert_eq!(pixel(&image, km1, 200), rgb_of("#f4efe6"), "above km 1");
}

#[test]
fn on_the_sample_the_tallest_bar_is_at_least_3_times_the_shortest() {
    let sample = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/sample.gpx");
    let xml = std::fs::read_to_string(sample).unwrap();
    let svg = svg_of(&xml, Variant::Bars);
    let heights = bar_heights(&svg);
    assert_eq!(heights.len(), 43, "{svg}");
    let (shortest, tallest) = heights
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &h| (lo.min(h), hi.max(h)));
    assert!(
        (80.0..=370.0).contains(&shortest) && (80.0..=370.0).contains(&tallest),
        "{heights:?}"
    );
    assert!(tallest >= 3.0 * shortest, "{heights:?}");
}

/// The `fill` of the first `<rect>` with `rx="6"`, a mosaic tile.
fn first_tile_fill(svg: &str) -> &str {
    let at = svg.find(r##"rx="6" fill="#"##).expect("a tile") + r#"rx="6" fill=""#.len();
    &svg[at..at + 7]
}

#[test]
fn mosaic_draws_one_tile_per_kilometre_a_ring_and_the_dashed_partial() {
    let xml = half();
    let svg = svg_of(&xml, Variant::Mosaic);
    assert_rendered(&svg);
    assert_eq!(split_count(&xml), 22);
    assert_eq!(svg.matches(r##"rx="6" fill="#"##).count(), 22, "{svg}");
    assert_eq!(svg.matches(r#"rx="6" fill="none""#).count(), 1, "{svg}");
    assert_eq!(svg.matches(r#"stroke-dasharray="3 3""#).count(), 1, "{svg}");
    assert!(svg.contains(r#"stroke-width="2.5""#), "{svg}");
    assert!(svg.contains(">0.10 km</text>"), "{svg}");
    assert!(svg.contains(r#"text-anchor="middle""#), "{svg}");
    // 22 tiles in 5 columns: 5 rows shrunk to 88 px to end at y = 660,
    // centred across the card.
    let first = r#"<rect x="120.0" y="180.0" width="88.0" height="88.0""#;
    assert!(svg.contains(first), "{svg}");
    let last = r#"<rect x="120.0" y="572.0" width="88.0" height="88.0""#;
    assert!(svg.contains(last), "{svg}");
    assert!(svg.contains(r#"font-size="36.96""#), "{svg}");
    assert!(svg.contains(r#"y="740""#), "{svg}");
    assert!(svg.contains(r#"y="790">SLOW</text>"#), "{svg}");
    assert!(svg.contains("FASTEST KM"), "{svg}");
    assert!(svg.contains(r#"<path d="M "#), "{svg}");
}

#[test]
fn the_mosaic_png_has_the_card_size_and_shows_the_tiles() {
    let pipeline = Pipeline::new().unwrap();
    let xml = half();
    let svg = svg_of(&xml, Variant::Mosaic);
    let image = decode(
        &pipeline
            .png(xml.as_bytes(), &request(Variant::Mosaic, 1.0))
            .unwrap(),
    );
    assert_eq!((image.0, image.1), (720, 1000));
    let large = pipeline
        .png(xml.as_bytes(), &request(Variant::Mosaic, 2.0))
        .unwrap();
    assert_eq!((decode(&large).0, decode(&large).1), (1440, 2000));

    // The top right of the first tile (120, 180, 88 px) is free of text.
    let fill = rgb_of(first_tile_fill(&svg));
    assert_eq!(pixel(&image, 195, 192), fill, "first tile");
    let paper = rgb_of("#f4efe6");
    assert_eq!(pixel(&image, 100, 192), paper, "left of the grid");
    assert_eq!(pixel(&image, 213, 192), paper, "gap after the first tile");
    assert_eq!(pixel(&image, 300, 664), paper, "below the grid");
}

#[test]
fn the_mosaic_sets_tile_text_in_ink_or_paper() {
    let svg = svg_of(&half(), Variant::Mosaic);
    assert!(svg.contains(r##"<g fill="#1b1b1b">"##), "{svg}");
    assert!(svg.contains(r##"<g fill="#f4efe6">"##), "{svg}");
}

#[test]
fn both_variants_take_their_colours_from_the_theme() {
    let xml = half();
    let analysis = splits::analyze(&gpx::parse(xml.as_bytes()).unwrap()).unwrap();
    let meta = Meta {
        runner: "Julia Brandt".into(),
        event: "Spreelauf".into(),
        city: None,
        date: time::macros::date!(2026 - 09 - 27),
    };
    let theme = Theme {
        paper: "#010203",
        ink: "#040506",
        muted: "#070809",
        accent: "#0a0b0c",
    };
    let renderer = Renderer::new().unwrap();
    for variant in [Variant::Bars, Variant::Mosaic] {
        let mut data = card::build(
            &analysis,
            &meta,
            &Palette::light(),
            &TextMeasure::new(),
            variant,
        );
        data.theme = theme;
        let svg = renderer.svg(&data).unwrap();
        for colour in ["#010203", "#040506", "#0a0b0c"] {
            assert!(svg.contains(colour), "{variant}: no {colour} in {svg}");
        }
        for literal in ["#f4efe6", "#1b1b1b"] {
            assert!(!svg.contains(literal), "{variant}: {literal} in {svg}");
        }
    }
}
