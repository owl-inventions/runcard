//! The `rings` and `route` variants end to end: synthetic GPX through the
//! pipeline to SVG markup and PNG.

mod common;

use runcard::palette::Palette;
use runcard::pipeline::{CardRequest, Pipeline};
use runcard::render::Renderer;
use runcard::text::TextMeasure;
use runcard::{card, format, gpx, splits, Analysis, Meta, Variant};
use time::macros::{date, datetime};
use time::OffsetDateTime;

const START: OffsetDateTime = datetime!(2026-09-27 09:00:00 UTC);

/// 10.46 km at 5:00 per km with a 4:30 km 7: ten full kilometres and a partial.
fn run() -> String {
    common::synthetic_gpx(10_460.0, |km| if km == 7 { 270.0 } else { 300.0 }, START)
}

fn analysis_of(xml: &str) -> Analysis {
    splits::analyze(&gpx::parse(xml.as_bytes()).unwrap()).unwrap()
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

fn svg_of(variant: Variant) -> String {
    Pipeline::new()
        .unwrap()
        .svg(run().as_bytes(), &request(variant, 1.0))
        .unwrap()
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

fn pixel((width, _, rgb): &(u32, u32, Vec<u8>), x: f32, y: f32) -> [u8; 3] {
    let i = ((y as u32 * width + x as u32) * 3) as usize;
    [rgb[i], rgb[i + 1], rgb[i + 2]]
}

fn rgb_of(hex: &str) -> [u8; 3] {
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).expect("a hex colour");
    [channel(1), channel(3), channel(5)]
}

fn assert_colour(actual: [u8; 3], hex: &str, what: &str) {
    let expected = rgb_of(hex);
    assert!(
        actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 2),
        "{what}: pixel {actual:?}, expected {hex}"
    );
}

/// `text` as the template writes it: the HTML auto-escape spells `/` as `&#x2f;`.
fn escaped(text: &str) -> String {
    text.replace('/', "&#x2f;")
}

/// The start tag of the `<text>` element whose content is `content`.
fn text_tag<'a>(svg: &'a str, content: &str) -> &'a str {
    let end = svg
        .find(&format!(">{content}</text>"))
        .unwrap_or_else(|| panic!("no text {content:?} in {svg}"));
    let start = svg[..end].rfind("<text").expect("a text start tag");
    &svg[start..=end]
}

/// The markup inside `<g id="{id}">`, which holds no nested group.
fn group<'a>(svg: &'a str, id: &str) -> &'a str {
    let open = format!(r#"<g id="{id}">"#);
    let start = svg
        .find(&open)
        .unwrap_or_else(|| panic!("no group {id} in {svg}"))
        + open.len();
    let end = start + svg[start..].find("</g>").expect("the group is closed");
    &svg[start..end]
}

/// The value of attribute `name` of every `<path` in `markup`.
fn path_attrs<'a>(markup: &'a str, name: &str) -> Vec<&'a str> {
    let key = format!(r#" {name}=""#);
    markup
        .split("<path")
        .skip(1)
        .map(|tag| {
            let start = tag.find(&key).expect("the attribute") + key.len();
            &tag[start..start + tag[start..].find('"').unwrap()]
        })
        .collect()
}

fn assert_template_filled(svg: &str) {
    assert!(svg.starts_with("<svg"), "{svg}");
    assert!(!svg.contains("{{") && !svg.contains("{%"), "{svg}");
}

fn assert_card_png(variant: Variant) {
    let pipeline = Pipeline::new().unwrap();
    for (scale, size) in [(1.0, (720, 1000)), (2.0, (1440, 2000))] {
        let png = pipeline
            .png(run().as_bytes(), &request(variant, scale))
            .unwrap();
        let (width, height, _) = decode(&png);
        assert_eq!((width, height), size, "{variant} at {scale}");
    }
}

#[test]
fn rings_draws_one_filled_segment_per_kilometre() {
    let svg = svg_of(Variant::Rings);
    assert_template_filled(&svg);
    let ring = group(&svg, "ring");
    assert_eq!(ring.matches("<path").count(), 11, "{ring}");
    assert!(!ring.contains("stroke"), "{ring}");
    let fills = path_attrs(ring, "fill");
    assert!(
        fills.iter().all(|f| f.starts_with('#') && f.len() == 7),
        "{fills:?}"
    );
}

#[test]
fn rings_shows_the_time_and_summary_inside_and_the_name_centred_below() {
    let svg = svg_of(Variant::Rings);
    let analysis = analysis_of(&run());
    let time = format::duration(analysis.elapsed_s);
    let pace = format::pace_caps(analysis.elapsed_s / analysis.distance_m * 1000.0);

    let time_tag = text_tag(&svg, &time);
    for attr in [
        r#"x="360""#,
        r#"y="432""#,
        r#"text-anchor="middle""#,
        r#"font-size="72""#,
    ] {
        assert!(time_tag.contains(attr), "{attr} missing in {time_tag}");
    }
    let summary = text_tag(&svg, &escaped(&format!("10.46 KM \u{b7} {pace}")));
    for attr in [
        r#"x="360""#,
        r#"y="470""#,
        r#"text-anchor="middle""#,
        r#"letter-spacing="3""#,
    ] {
        assert!(summary.contains(attr), "{attr} missing in {summary}");
    }
    let name = text_tag(&svg, "JULIA BRANDT");
    for attr in [
        r#"x="360""#,
        r#"y="740""#,
        r#"text-anchor="middle""#,
        r#"font-size="76.0""#,
    ] {
        assert!(name.contains(attr), "{attr} missing in {name}");
    }
}

#[test]
fn rings_has_no_stripe_bar_and_the_bottom_block_of_classic() {
    let svg = svg_of(Variant::Rings);
    let analysis = analysis_of(&run());
    assert!(!svg.contains(r#"height="380""#), "{svg}");

    // Legend, fastest line left-aligned, rule and stats row as on classic,
    // the legend and the fastest line 30 and 18 px lower.
    assert!(svg.contains(r#"<text x="32" y="790">SLOW</text>"#), "{svg}");
    assert!(svg.contains(r#"<rect x="104" y="779" width="22""#), "{svg}");
    let fastest = text_tag(&svg, &escaped("FASTEST KM 7 \u{b7} 4:30 /KM"));
    assert!(fastest.contains(r#"x="32" y="830""#), "{fastest}");
    assert!(!fastest.contains("text-anchor"), "{fastest}");
    assert!(
        svg.contains(r#"<line x1="32" y1="860" x2="688" y2="860""#),
        "the rule at 860: {svg}"
    );
    for (x, label) in [(32, "DISTANCE"), (272, "TIME"), (512, "PACE")] {
        let tag = format!(r#"<text x="{x}" y="900">{label}</text>"#);
        assert!(svg.contains(&tag), "{tag} missing in {svg}");
    }
    let values = [
        (32, format::distance(analysis.distance_m)),
        (272, format::duration(analysis.elapsed_s)),
        (
            512,
            format::pace(analysis.elapsed_s / analysis.distance_m * 1000.0),
        ),
    ];
    for (x, value) in values {
        let tag = format!(r#"<text x="{x}" y="948">{}</text>"#, escaped(&value));
        assert!(svg.contains(&tag), "{tag} missing in {svg}");
    }
    assert!(
        svg.contains(r#"font-family="Bebas Neue" font-size="44""#),
        "{svg}"
    );

    // The route outline and its start dot sit in the 100 x 70 box at (588, 770).
    let route = svg
        .split("<path")
        .find(|tag| tag.contains(r#"fill="none""#))
        .expect("the route outline");
    let d = &route[route.find(r#"d=""#).unwrap() + 3..];
    let numbers: Vec<f32> = d[..d.find('"').unwrap()]
        .split(' ')
        .filter_map(|t| t.parse().ok())
        .collect();
    assert!(numbers.len() >= 4, "{route}");
    for xy in numbers.chunks(2) {
        assert!((588.0..=688.0).contains(&xy[0]), "x {} in {route}", xy[0]);
        assert!((770.0..=840.0).contains(&xy[1]), "y {} in {route}", xy[1]);
    }
    let start = svg.split("<circle").find(|tag| tag.contains(r#"r="3.5""#));
    let start = start.expect("the start dot");
    let cy = &start[start.find(r#"cy=""#).unwrap() + 4..];
    let cy: f32 = cy[..cy.find('"').unwrap()].parse().unwrap();
    assert!((770.0..=840.0).contains(&cy), "{start}");
}

#[test]
fn the_rings_png_shows_the_first_kilometre_in_its_colour() {
    let pipeline = Pipeline::new().unwrap();
    let rendered = pipeline
        .render(run().as_bytes(), &request(Variant::Rings, 1.0))
        .unwrap();
    let image = decode(&rendered.png);
    assert_eq!((image.0, image.1), (720, 1000));
    let first = path_attrs(group(&rendered.svg, "ring"), "fill")[0].to_owned();
    // Mid-angle of km 1 of 10.46, halfway between the radii.
    let mid = (360.0_f32 / 10.46 / 2.0).to_radians();
    let r = (236.0 + 150.0) / 2.0;
    let (x, y) = (360.0 + r * mid.sin(), 440.0 - r * mid.cos());
    assert_colour(pixel(&image, x, y), &first, "km 1");
    assert_colour(
        pixel(&image, 360.0, 440.0 - r),
        "#f4efe6",
        "the gap at 12 o'clock",
    );
}

#[test]
fn the_rings_card_has_the_size_of_the_scale() {
    assert_card_png(Variant::Rings);
}

#[test]
fn route_draws_one_thick_stroke_per_kilometre_in_the_map() {
    let svg = svg_of(Variant::Route);
    assert_template_filled(&svg);
    let map = group(&svg, "map");
    assert_eq!(map.matches("<path").count(), 11, "{map}");
    assert_eq!(map.matches(r#"stroke-width="7""#).count(), 11, "{map}");
    assert_eq!(
        map.matches(r#"stroke-linecap="round""#).count(),
        11,
        "{map}"
    );
    assert_eq!(map.matches(r#"fill="none""#).count(), 11, "{map}");
    let strokes = path_attrs(map, "stroke");
    let distinct: std::collections::BTreeSet<&&str> = strokes.iter().collect();
    assert!(distinct.len() >= 2, "km 7 has its own colour: {strokes:?}");
}

#[test]
fn route_marks_the_start_and_the_finish() {
    let map_svg = svg_of(Variant::Route);
    let map = group(&map_svg, "map");
    assert!(map.contains(r##"r="6" fill="#1b1b1b""##), "{map}");
    assert!(map.contains(r##"r="3" fill="#f4efe6""##), "{map}");
    assert!(
        map.contains(r##"width="10" height="10" fill="#1b1b1b""##),
        "{map}"
    );
}

#[test]
fn route_has_a_compact_strip_with_the_fastest_dot_and_the_stats_row() {
    let svg = svg_of(Variant::Route);
    let strip = group(&svg, "strip");
    assert_eq!(strip.matches(r#"y="640""#).count(), 11, "{strip}");
    assert_eq!(strip.matches(r#"height="28""#).count(), 11, "{strip}");
    assert!(svg.contains(r#"cy="632" r="3""#), "{svg}");
    assert!(!svg.contains(r#"height="380""#), "{svg}");
    let name = text_tag(&svg, "JULIA BRANDT");
    assert!(name.contains(r#"x="32" y="740""#), "{name}");
    let fastest = text_tag(&svg, &escaped("FASTEST KM 7 \u{b7} 4:30 /KM"));
    assert!(fastest.contains(r#"y="830""#), "{fastest}");
    assert!(svg.contains(">DISTANCE</text>"), "{svg}");
}

#[test]
fn the_route_png_shows_the_strip_in_the_kilometre_colours() {
    let pipeline = Pipeline::new().unwrap();
    let rendered = pipeline
        .render(run().as_bytes(), &request(Variant::Route, 1.0))
        .unwrap();
    let image = decode(&rendered.png);
    let strip = group(&rendered.svg, "strip");
    let rect = strip.split("<rect").nth(1).unwrap();
    let attr = |name: &str| {
        let key = format!(r#" {name}=""#);
        let start = rect.find(&key).unwrap() + key.len();
        rect[start..start + rect[start..].find('"').unwrap()].to_owned()
    };
    let (x, width) = (
        attr("x").parse::<f32>().unwrap(),
        attr("width").parse::<f32>().unwrap(),
    );
    assert_colour(
        pixel(&image, x + width / 2.0, 654.0),
        &attr("fill"),
        "km 1 band",
    );
}

#[test]
fn the_route_card_has_the_size_of_the_scale() {
    assert_card_png(Variant::Route);
}

#[test]
fn a_route_without_two_distinct_points_leaves_the_map_empty() {
    let analysis = Analysis {
        points: vec![analysis_of(&run()).points[0].clone(); 2],
        ..analysis_of(&run())
    };
    let meta = Meta {
        runner: "Julia Brandt".into(),
        event: "Spreelauf".into(),
        city: None,
        date: date!(2026 - 09 - 27),
    };
    let data = card::build(
        &analysis,
        &meta,
        &Palette::light(),
        &TextMeasure::new(),
        Variant::Route,
    );
    let renderer = Renderer::new().unwrap();
    let svg = renderer.svg(&data).unwrap();
    assert_template_filled(&svg);
    let map = group(&svg, "map");
    assert!(!map.contains("<path"), "{map}");
    assert!(!map.contains("<circle") && !map.contains("<rect"), "{map}");
    assert_eq!(group(&svg, "strip").matches("<rect").count(), 11);
    let (width, height, _) = decode(&renderer.png(&svg, 1.0).unwrap());
    assert_eq!((width, height), (720, 1000));
}
