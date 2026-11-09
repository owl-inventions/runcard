//! The `spiral`, `trail` and `dots` variants end to end: a synthetic run
//! through the pipeline to SVG markup and PNG, in both themes.

mod common;

use std::ops::Range;

use common::{Course, Shape};
use runcard::pipeline::{CardRequest, Pipeline};
use runcard::{Theme, Variant};
use time::macros::datetime;
use time::OffsetDateTime;

const START: OffsetDateTime = datetime!(2026-04-19 07:30:00 UTC);

/// A loop along the harbour: straight legs through these waypoints (east,
/// north) and back to the first.
const HARBOUR: Course = Course {
    name: "Harbour Half",
    start: (38.7075, -9.1365),
    step_m: 25.0,
    shape: Shape::Waypoints(&[(0.0, 0.0), (6.0, -1.0), (8.0, 2.0), (5.0, 4.0), (1.0, 3.0)]),
};

/// 12.4 km: twelve full kilometres and a 0.4 km partial, km 9 the fastest.
const DISTANCE_M: f64 = 12_400.0;
const PACES: &[f64] = &[
    330.0, 325.0, 320.0, 318.0, 322.0, 335.0, 340.0, 300.0, 280.0, 310.0, 315.0, 305.0, 300.0,
];
/// Splits of the run: twelve full kilometres and the partial.
const SPLITS: usize = 13;

const VARIANTS: [Variant; 3] = [Variant::Spiral, Variant::Trail, Variant::Dots];

fn run() -> String {
    common::course_gpx(&HARBOUR, DISTANCE_M, common::paces(PACES), START)
}

fn request(variant: Variant, theme: Theme) -> CardRequest {
    CardRequest {
        runner: "Mara Lindqvist".into(),
        event: None,
        city: Some("Lisbon".into()),
        date: None,
        scale: 1.0,
        variant,
        theme,
    }
}

fn svg_of(variant: Variant, theme: Theme) -> String {
    Pipeline::new()
        .unwrap()
        .svg(run().as_bytes(), &request(variant, theme))
        .unwrap()
}

/// Width, height and RGB bytes of a PNG.
struct Image {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
}

impl Image {
    fn decode(png_bytes: &[u8]) -> Self {
        let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
        let mut reader = decoder.read_info().expect("a valid png header");
        let mut buf = vec![0; reader.output_buffer_size().expect("a sane png size")];
        let info = reader.next_frame(&mut buf).expect("a valid png frame");
        assert_eq!(info.color_type, png::ColorType::Rgb);
        buf.truncate(info.buffer_size());
        Self {
            width: info.width,
            height: info.height,
            rgb: buf,
        }
    }

    fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * self.width + x) * 3) as usize;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }

    /// How many pixels in the columns `x` of the rows `y` differ from `paper`
    /// by more than 2 in any channel.
    fn inked(&self, x: Range<u32>, y: Range<u32>, paper: [u8; 3]) -> usize {
        y.flat_map(|y| x.clone().map(move |x| (x, y)))
            .filter(|&(x, y)| {
                self.pixel(x, y)
                    .iter()
                    .zip(paper)
                    .any(|(a, b)| a.abs_diff(b) > 2)
            })
            .count()
    }
}

fn rgb_of(hex: &str) -> [u8; 3] {
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).expect("a hex colour");
    [channel(1), channel(3), channel(5)]
}

/// The markup before the legend: the visualisation (twice on dark paper),
/// without the route preview the frame draws after the legend.
fn picture(svg: &str) -> &str {
    &svg[..svg.find(">slower</text>").expect("the legend")]
}

/// Every start tag `<{name} ...>` in `svg`.
fn tags<'a>(svg: &'a str, name: &str) -> Vec<&'a str> {
    let open = format!("<{name} ");
    svg.match_indices(&open)
        .map(|(start, _)| &svg[start..start + svg[start..].find('>').expect("a closed tag")])
        .collect()
}

/// The value of attribute `name` in the start tag `tag`.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(r#" {name}=""#);
    let start = tag.find(&key)? + key.len();
    Some(&tag[start..start + tag[start..].find('"')?])
}

#[test]
fn each_variant_fills_its_template_and_renders_a_720_by_1000_card_at_scale_1() {
    let pipeline = Pipeline::new().unwrap();
    for variant in VARIANTS {
        for theme in Theme::ALL {
            let what = format!("{variant} {theme}");
            let rendered = pipeline
                .render(run().as_bytes(), &request(variant, theme))
                .unwrap();
            let svg = &rendered.svg;
            assert!(svg.starts_with("<svg"), "{what}: {svg}");
            assert!(!svg.contains("{{") && !svg.contains("{%"), "{what}: {svg}");
            assert!(svg.contains(">Harbour Half</text>"), "{what}");
            assert_eq!(
                svg.contains("feGaussianBlur"),
                theme == Theme::Dark,
                "{what}"
            );
            let image = Image::decode(&rendered.png);
            assert_eq!((image.width, image.height), (720, 1000), "{what}");
        }
    }
}

#[test]
fn the_dark_theme_draws_each_visualisation_twice_and_the_light_theme_once() {
    for variant in VARIANTS {
        let light = svg_of(variant, Theme::Light);
        let dark = svg_of(variant, Theme::Dark);
        let drawn = |svg: &str| {
            let picture = picture(svg);
            tags(picture, "path").len() + tags(picture, "circle").len()
        };
        assert!(drawn(&light) > SPLITS, "{variant}: {light}");
        assert_eq!(drawn(&dark), 2 * drawn(&light), "{variant}");
        // No id inside the visualisation, which the glow repeats: only the filter has one.
        assert_eq!(light.matches(" id=\"").count(), 0, "{variant}");
        assert_eq!(dark.matches(" id=\"").count(), 1, "{variant}");
    }
}

#[test]
fn spiral_draws_one_butt_capped_segment_per_kilometre_with_its_markers() {
    let card = svg_of(Variant::Spiral, Theme::Light);
    let svg = picture(&card);
    let paths = tags(svg, "path");
    assert_eq!(paths.len(), SPLITS, "{svg}");
    let width = attr(paths[0], "stroke-width").expect("a stroke width");
    for path in &paths {
        assert_eq!(attr(path, "fill"), Some("none"), "{path}");
        assert_eq!(attr(path, "stroke-linecap"), Some("butt"), "{path}");
        assert_eq!(attr(path, "stroke-linejoin"), Some("round"), "{path}");
        assert_eq!(attr(path, "stroke-width"), Some(width), "{path}");
        let stroke = attr(path, "stroke").unwrap();
        assert!(stroke.starts_with('#') && stroke.len() == 7, "{path}");
    }
    let width: f32 = width.parse().unwrap();
    assert!((4.0..=16.0).contains(&width), "{width}");

    let ink = Theme::Light.tokens().ink;
    let circles = tags(svg, "circle");
    assert_eq!(
        circles.len(),
        2,
        "the start dot and the fastest ring: {circles:?}"
    );
    let dot = circles.iter().find(|c| attr(c, "r") == Some("3")).unwrap();
    assert_eq!(attr(dot, "fill"), Some(ink));
    assert_eq!(
        (attr(dot, "cx"), attr(dot, "cy")),
        (Some("384"), Some("440"))
    );
    let ring = circles.iter().find(|c| attr(c, "r") != Some("3")).unwrap();
    assert_eq!(attr(ring, "fill"), Some("none"));
    assert_eq!(attr(ring, "stroke"), Some(ink));
    assert_eq!(attr(ring, "stroke-width"), Some("1.5"));
    let r: f32 = attr(ring, "r").unwrap().parse().unwrap();
    assert!((r - (width / 2.0 + 4.0)).abs() < 0.01, "{ring}");

    let squares: Vec<&str> = tags(svg, "rect")
        .into_iter()
        .filter(|r| attr(r, "width") == Some("6"))
        .collect();
    assert_eq!(squares.len(), 1, "{squares:?}");
    assert_eq!(attr(squares[0], "height"), Some("6"));
    assert_eq!(attr(squares[0], "fill"), Some(ink));
}

#[test]
fn trail_draws_one_round_stroke_per_kilometre_and_the_start_and_finish() {
    let svg = svg_of(Variant::Trail, Theme::Light);
    let paths = tags(&svg, "path");
    assert_eq!(paths.len(), SPLITS, "{svg}");
    for path in &paths {
        assert_eq!(attr(path, "fill"), Some("none"), "{path}");
        assert_eq!(attr(path, "stroke-width"), Some("6"), "{path}");
        assert_eq!(attr(path, "stroke-linecap"), Some("round"), "{path}");
        assert_eq!(attr(path, "stroke-linejoin"), Some("round"), "{path}");
    }
    // Each kilometre starts where the one before ends.
    let ends: Vec<(&str, &str)> = paths
        .iter()
        .map(|p| {
            let d = attr(p, "d").unwrap();
            let first = d.split(" L ").next().unwrap().trim_start_matches("M ");
            (first, d.rsplit(" L ").next().unwrap())
        })
        .collect();
    for pair in ends.windows(2) {
        assert_eq!(pair[0].1, pair[1].0, "{pair:?}");
    }

    let tokens = Theme::Light.tokens();
    let circles = tags(&svg, "circle");
    assert_eq!(circles.len(), 1, "{circles:?}");
    let start = circles[0];
    assert_eq!(attr(start, "r"), Some("5"));
    assert_eq!(attr(start, "fill"), Some(tokens.paper));
    assert_eq!(attr(start, "stroke"), Some(tokens.ink));
    assert_eq!(attr(start, "stroke-width"), Some("2"));
    let squares: Vec<&str> = tags(&svg, "rect")
        .into_iter()
        .filter(|r| attr(r, "width") == Some("7"))
        .collect();
    assert_eq!(squares.len(), 1, "{squares:?}");
    assert_eq!(attr(squares[0], "height"), Some("7"));
    assert_eq!(attr(squares[0], "fill"), Some(tokens.ink));
}

#[test]
fn dots_draws_one_numbered_circle_per_kilometre_and_rings_the_fastest() {
    let svg = svg_of(Variant::Dots, Theme::Light);
    let tokens = Theme::Light.tokens();
    let circles = tags(picture(&svg), "circle");
    assert_eq!(circles.len(), SPLITS + 1, "{svg}");
    let rings: Vec<&&str> = circles
        .iter()
        .filter(|c| attr(c, "fill") == Some("none"))
        .collect();
    assert_eq!(rings.len(), 1, "{rings:?}");
    assert_eq!(attr(rings[0], "stroke"), Some(tokens.ink));
    assert_eq!(attr(rings[0], "stroke-width"), Some("1.5"));
    let faded: Vec<&&str> = circles
        .iter()
        .filter(|c| attr(c, "opacity") == Some("0.6"))
        .collect();
    assert_eq!(faded.len(), 1, "only the partial: {faded:?}");

    // 13 cells of 89 px: every kilometre is numbered, in muted mono 11 px,
    // after the dots and the ring and with a paper halo, so the ring around
    // a fast kilometre in the corner of its cell does not cross its number.
    let ring = svg.find(r#"fill="none""#).expect("the ring");
    for km in 1..=SPLITS {
        let end = svg
            .find(&format!(">{km}</text>"))
            .unwrap_or_else(|| panic!("no number {km} in {svg}"));
        let start = svg[..end].rfind("<text").unwrap();
        let tag = &svg[start..end];
        assert!(start > ring, "number {km} is drawn before the ring");
        assert!(tag.contains(r#"font-family="JetBrains Mono""#), "{tag}");
        assert_eq!(attr(tag, "font-size"), Some("11"), "{tag}");
        assert_eq!(attr(tag, "fill"), Some(tokens.muted), "{tag}");
        assert_eq!(attr(tag, "stroke"), Some(tokens.paper), "{tag}");
        assert_eq!(attr(tag, "paint-order"), Some("stroke"), "{tag}");
    }
}

#[test]
fn nothing_is_drawn_between_the_title_block_and_the_zone_or_the_zone_and_the_legend() {
    let pipeline = Pipeline::new().unwrap();
    let paper = rgb_of(Theme::Light.tokens().paper);
    for variant in VARIANTS {
        let png = pipeline
            .png(run().as_bytes(), &request(variant, Theme::Light))
            .unwrap();
        let image = Image::decode(&png);
        assert_eq!(
            image.inked(0..720, 126..158, paper),
            0,
            "{variant} above the zone"
        );
        // The route preview, start marker included, inks at most x 469.5 to
        // 674.5 and y 731.5 to 792.5 (none for trail).
        assert_eq!(
            image.inked(0..720, 722..731, paper),
            0,
            "{variant} below the zone"
        );
        assert_eq!(
            image.inked(0..469, 731..756, paper),
            0,
            "{variant} below the zone, left of the route preview"
        );
        assert!(
            image.inked(0..720, 160..720, paper) > 2000,
            "{variant} draws in the zone"
        );
    }
}
