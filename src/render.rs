//! Renders the card: fills the variant's SVG template with minijinja, rasterises it with resvg, encodes the PNG.

use std::sync::Arc;

use minijinja::{context, AutoEscape, Environment, Value};

use crate::error::{Error, Result};
use crate::layout::{bars::Bars, mosaic::Mosaic, rings::Rings, route_hero::RouteHero};
use crate::model::{CardData, Theme};
use crate::route;
use crate::text::Font;
use crate::variant::Variant;

/// The file name and the source of the template of `variant`, compiled in.
fn template(variant: Variant) -> (&'static str, &'static str) {
    match variant {
        Variant::Classic => ("classic.svg", include_str!("../templates/classic.svg")),
        Variant::Noir => ("noir.svg", include_str!("../templates/noir.svg")),
        Variant::Rings => ("rings.svg", include_str!("../templates/rings.svg")),
        Variant::Route => ("route.svg", include_str!("../templates/route.svg")),
        Variant::Bars => ("bars.svg", include_str!("../templates/bars.svg")),
        Variant::Mosaic => ("mosaic.svg", include_str!("../templates/mosaic.svg")),
    }
}

/// Template environment and SVG options with the bundled fonts, built once.
pub struct Renderer {
    env: Environment<'static>,
    opts: usvg::Options<'static>,
}

impl Renderer {
    /// Loads the templates of every variant and the fonts.
    pub fn new() -> Result<Self> {
        let mut env = Environment::new();
        env.set_auto_escape_callback(|_| AutoEscape::Html);
        for variant in Variant::ALL {
            let (name, source) = template(variant);
            env.add_template(name, source)?;
        }

        let mut fontdb = fontdb::Database::new();
        for font in Font::ALL {
            fontdb.load_font_source(fontdb::Source::Binary(Arc::new(font.data())));
        }
        let opts = usvg::Options {
            fontdb: Arc::new(fontdb),
            ..usvg::Options::default()
        };
        Ok(Self { env, opts })
    }

    /// The card as SVG markup, from the template of `data.variant`.
    pub fn svg(&self, data: &CardData) -> Result<String> {
        let template = self.env.get_template(&format!("{}.svg", data.variant))?;
        Ok(template.render(template_context(data))?)
    }

    /// Rasterises SVG markup at `scale` times its size and encodes it as RGB PNG.
    pub fn png(&self, svg: &str, scale: f32) -> Result<Vec<u8>> {
        let tree = usvg::Tree::from_str(svg, &self.opts).map_err(|e| Error::Svg(e.to_string()))?;
        let size = tree.size();
        let width = (size.width() * scale).round() as u32;
        let height = (size.height() * scale).round() as u32;
        let mut pixmap = tiny_skia::Pixmap::new(width, height)
            .ok_or_else(|| Error::Svg(format!("cannot rasterise at {width}x{height}")))?;
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        encode(&pixmap)
    }

    /// The card as PNG bytes at `scale` times the 720x1000 card size.
    pub fn render(&self, data: &CardData, scale: f32) -> Result<Vec<u8>> {
        self.png(&self.svg(data)?, scale)
    }
}

/// The template context: the fields of `CardData`, with the route as SVG path
/// data (`route_path`) and the user-given texts (header, title, runner) cleared
/// of characters XML forbids. Built by hand because minijinja's `serde`
/// feature is off; each shape block builds its own value.
fn template_context(data: &CardData) -> Value {
    let stripes: Vec<Value> = data
        .stripes
        .iter()
        .map(|s| context! { width => px(s.width), color => &s.color })
        .collect();
    let splits: Vec<Value> = data
        .splits
        .iter()
        .map(|s| {
            context! {
                index => s.index,
                pace => &s.pace,
                color => &s.color,
                partial => s.partial,
                fraction => px(s.fraction),
                t => px(s.t),
            }
        })
        .collect();
    let ticks: Vec<Value> = data
        .ticks
        .iter()
        .map(|&(x, km)| Value::from(vec![px(x), Value::from(km)]))
        .collect();
    let point = |(x, y): (f32, f32)| Value::from(vec![px(x), px(y)]);
    context! {
        header => xml_chars(&data.header),
        date => &data.date,
        title => xml_chars(&data.title),
        title_size => px(data.title_size),
        runner => xml_chars(&data.runner),
        runner_size => px(data.runner_size),
        stripes => stripes,
        fastest_x => data.fastest_x.map(px),
        ticks => ticks,
        route_start => point(data.route_start),
        route_path => route::to_path(&data.route),
        legend => &data.legend,
        fastest_line => &data.fastest_line,
        distance => &data.distance,
        time => &data.time,
        pace => &data.pace,
        variant => data.variant,
        theme => theme_value(&data.theme),
        splits => splits,
        avg_t => px(data.avg_t),
        avg_pace => &data.avg_pace,
        rings => data.rings.as_ref().map(Rings::to_value),
        route_hero => data.route_hero.as_ref().map(RouteHero::to_value),
        bars => data.bars.as_ref().map(Bars::to_value),
        mosaic => data.mosaic.as_ref().map(Mosaic::to_value),
    }
}

fn theme_value(theme: &Theme) -> Value {
    context! {
        paper => theme.paper,
        ink => theme.ink,
        muted => theme.muted,
        accent => theme.accent,
    }
}

/// `text` without the characters XML 1.0 forbids (most C0 controls, U+FFFE
/// and U+FFFF), which would make the SVG unparsable.
fn xml_chars(text: &str) -> String {
    text.chars()
        .filter(|&c| matches!(c, '\t' | '\n' | '\r' | ' '..='\u{fffd}' | '\u{10000}'..))
        .collect()
}

/// A card coordinate rounded to two decimals, so the SVG reads `13.3`, not
/// the `13.300000190734863` of a widened `f32`.
pub(crate) fn px(v: f32) -> Value {
    Value::from((f64::from(v) * 100.0).round() / 100.0)
}

/// Encodes the premultiplied RGBA pixmap as an 8-bit RGB PNG.
fn encode(pixmap: &tiny_skia::Pixmap) -> Result<Vec<u8>> {
    let rgb: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue()]
        })
        .collect();
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, pixmap.width(), pixmap.height());
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&rgb)?;
    writer.finish()?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card;
    use crate::model::{Analysis, Meta, Split, SplitView, Stripe, Theme, TrackPoint};
    use crate::palette::{anchors, pace_position, Palette};
    use crate::text::TextMeasure;
    use crate::variant::Variant;

    const STRIPE_WIDTH: f32 = (656.0 - 42.0 * 2.0) / 43.0;

    /// Marathon-like paces in s/km: a steady 5:40, a fast 30th km, a slower finish.
    fn paces() -> Vec<f64> {
        (1..=43)
            .map(|km| match km {
                30 => 316.0,
                1 => 352.0,
                36..=42 => 350.0 + f64::from(km - 36) * 4.0,
                43 => 330.0,
                _ => 335.0 + f64::from(km % 5) * 3.0,
            })
            .collect()
    }

    fn stripe_x(index: usize) -> f32 {
        32.0 + index as f32 * (STRIPE_WIDTH + 2.0)
    }

    fn fixture(runner: &str) -> CardData {
        let palette = Palette::light();
        let paces = paces();
        let (slow, fast) = anchors(&paces);
        let stripes = paces
            .iter()
            .map(|&pace| Stripe {
                width: STRIPE_WIDTH,
                color: palette.hex(pace_position(pace, slow, fast)),
            })
            .collect();
        let route = vec![
            (588.0, 616.0),
            (688.0, 616.0),
            (688.0, 716.0),
            (588.0, 716.0),
            (588.0, 616.0),
        ];
        CardData {
            header: "RUN · BERLIN".into(),
            date: "SEPTEMBER 27, 2026".into(),
            title: "Berlin Marathon".into(),
            title_size: 48.0,
            runner: runner.into(),
            runner_size: 76.0,
            stripes,
            fastest_x: Some(stripe_x(29) + STRIPE_WIDTH / 2.0),
            ticks: (1..=8)
                .map(|i| (stripe_x(i * 5 - 1) + STRIPE_WIDTH / 2.0, i as u32 * 5))
                .collect(),
            route_start: route[0],
            route,
            legend: palette.legend(5),
            fastest_line: "FASTEST KM 30 · 5:16 /KM".into(),
            distance: "42.195 km".into(),
            time: "3:58:41".into(),
            pace: "5:39 /km".into(),
            variant: "classic",
            theme: Theme::light(),
            splits: Vec::new(),
            avg_t: 0.5,
            avg_pace: "5:39".into(),
            rings: None,
            route_hero: None,
            bars: None,
            mosaic: None,
        }
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

    fn pixel((width, _, rgb): &(u32, u32, Vec<u8>), x: u32, y: u32) -> [u8; 3] {
        let i = ((y * width + x) * 3) as usize;
        [rgb[i], rgb[i + 1], rgb[i + 2]]
    }

    fn rgb_of(hex: &str) -> [u8; 3] {
        let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).expect("a hex colour");
        [channel(1), channel(3), channel(5)]
    }

    #[test]
    fn the_svg_shows_the_runner_in_capitals() {
        let svg = Renderer::new()
            .unwrap()
            .svg(&fixture("Mira Okafor"))
            .unwrap();
        assert!(svg.contains("MIRA OKAFOR"), "{svg}");
    }

    /// The start tag of the `<text>` element whose content is `content`.
    fn text_tag<'a>(svg: &'a str, content: &str) -> &'a str {
        let end = svg
            .find(&format!(">{content}</text>"))
            .unwrap_or_else(|| panic!("no text {content:?} in {svg}"));
        let start = svg[..end].rfind("<text").expect("a text start tag");
        &svg[start..=end]
    }

    #[test]
    fn the_svg_sets_the_name_and_the_title_at_their_fitted_sizes() {
        let data = CardData {
            runner_size: 52.3,
            title_size: 31.5,
            ..fixture("Mira Okafor")
        };
        let svg = Renderer::new().unwrap().svg(&data).unwrap();
        let runner = text_tag(&svg, "MIRA OKAFOR");
        assert!(runner.contains(r#"font-size="52.3""#), "{runner}");
        let title = text_tag(&svg, "Berlin Marathon");
        assert!(title.contains(r#"font-size="31.5""#), "{title}");
    }

    #[test]
    fn the_svg_has_a_background_a_rect_per_stripe_and_five_legend_swatches() {
        let svg = Renderer::new()
            .unwrap()
            .svg(&fixture("Mira Okafor"))
            .unwrap();
        assert_eq!(svg.matches("<rect").count(), 1 + 43 + 5, "{svg}");
    }

    #[test]
    fn the_svg_escapes_markup_in_the_names() {
        let svg = Renderer::new()
            .unwrap()
            .svg(&fixture("Tom & Jerry"))
            .unwrap();
        assert!(svg.contains("TOM &amp; JERRY"), "{svg}");
        assert!(!svg.contains("TOM & JERRY"), "{svg}");
    }

    #[test]
    fn xml_chars_drops_what_xml_forbids_and_keeps_the_rest() {
        assert_eq!(
            xml_chars("a\u{0}b\u{1}c\u{b}d\u{c}e\u{1b}f\u{1f}g\u{fffe}h\u{ffff}i"),
            "abcdefghi"
        );
        let allowed = "\t\n\r Zürich \u{7f}\u{d7ff}\u{e000}\u{fffd}\u{10000}\u{10ffff}";
        assert_eq!(xml_chars(allowed), allowed);
    }

    #[test]
    fn a_control_character_in_the_names_does_not_break_the_png() {
        let data = CardData {
            header: "RUN \u{b7} BER\u{1b}LIN".into(),
            title: "Berlin\u{b} Marathon\u{fffe}".into(),
            ..fixture("Mira\u{1} Okafor")
        };
        let renderer = Renderer::new().unwrap();
        let svg = renderer.svg(&data).unwrap();
        assert!(svg.contains(">MIRA OKAFOR</text>"), "{svg}");
        assert!(svg.contains(">Berlin Marathon</text>"), "{svg}");
        assert!(svg.contains(">RUN \u{b7} BERLIN</text>"), "{svg}");
        renderer.png(&svg, 1.0).expect("the svg rasterises");
    }

    #[test]
    fn the_png_has_the_card_size_times_the_scale() {
        let renderer = Renderer::new().unwrap();
        let data = fixture("Mira Okafor");
        let (w, h, _) = decode(&renderer.render(&data, 1.0).unwrap());
        assert_eq!((w, h), (720, 1000));
        let (w, h, _) = decode(&renderer.render(&data, 2.0).unwrap());
        assert_eq!((w, h), (1440, 2000));
    }

    #[test]
    fn the_png_shows_the_background_and_the_stripe_colours() {
        let data = fixture("Mira Okafor");
        let image = decode(&Renderer::new().unwrap().render(&data, 1.0).unwrap());
        assert_eq!(pixel(&image, 5, 5), rgb_of("#f4efe6"));
        let centre_x = (stripe_x(0) + STRIPE_WIDTH / 2.0) as u32;
        let actual = pixel(&image, centre_x, 370);
        let expected = rgb_of(&data.stripes[0].color);
        for (a, e) in actual.iter().zip(expected) {
            assert!(
                a.abs_diff(e) <= 2,
                "pixel {actual:?}, stripe colour {expected:?}"
            );
        }
    }

    #[test]
    fn the_png_is_compressed() {
        let png = Renderer::new()
            .unwrap()
            .render(&fixture("Mira Okafor"), 1.0)
            .unwrap();
        let raw_rgb = 720 * 1000 * 3;
        assert!(png.len() < raw_rgb / 4, "{} bytes", png.len());
    }

    #[test]
    fn unparsable_svg_is_an_svg_error() {
        let error = Renderer::new().unwrap().png("<svg", 1.0).unwrap_err();
        assert!(matches!(error, crate::Error::Svg(_)), "{error:?}");
    }

    /// The fixture as `variant` would get it: its name and its theme.
    fn fixture_as(variant: Variant) -> CardData {
        CardData {
            variant: variant.name(),
            theme: variant.theme(),
            ..fixture("Mira Okafor")
        }
    }

    #[test]
    fn every_variant_renders_its_own_template_on_its_theme() {
        let renderer = Renderer::new().unwrap();
        for variant in Variant::ALL {
            let data = fixture_as(variant);
            let svg = renderer.svg(&data).unwrap();
            assert!(svg.starts_with("<svg"), "{variant}: {svg}");
            assert!(
                !svg.contains("{{") && !svg.contains("{%"),
                "{variant}: {svg}"
            );
            assert!(svg.contains(">MIRA OKAFOR</text>"), "{variant}: {svg}");
            let paper = format!(r#"fill="{}""#, data.theme.paper);
            assert!(svg.contains(&paper), "{variant} has no {paper}: {svg}");
            let image = decode(&renderer.png(&svg, 1.0).unwrap());
            assert_eq!((image.0, image.1), (720, 1000), "{variant}");
            assert_eq!(pixel(&image, 2, 2), rgb_of(data.theme.paper), "{variant}");
        }
    }

    #[test]
    fn noir_blurs_a_copy_of_the_stripes_on_dark_paper() {
        let renderer = Renderer::new().unwrap();
        let noir = renderer.svg(&fixture_as(Variant::Noir)).unwrap();
        assert!(
            noir.contains(r#"<feGaussianBlur stdDeviation="14"/>"#),
            "{noir}"
        );
        assert!(
            noir.contains(r#"<g filter="url(#glow)" opacity="0.35">"#),
            "{noir}"
        );
        assert!(noir.contains(r##"fill="#0f1115""##), "{noir}");
        assert_eq!(
            noir.matches(r#"height="380" rx="2""#).count(),
            2 * 43,
            "{noir}"
        );
        let classic = renderer.svg(&fixture_as(Variant::Classic)).unwrap();
        assert!(!classic.contains("feGaussianBlur"), "{classic}");
        assert!(!classic.contains("#0f1115"), "{classic}");
    }

    #[test]
    fn the_noir_png_shows_the_stripes_over_their_glow() {
        let data = fixture_as(Variant::Noir);
        let image = decode(&Renderer::new().unwrap().render(&data, 1.0).unwrap());
        let centre_x = (stripe_x(0) + STRIPE_WIDTH / 2.0) as u32;
        let actual = pixel(&image, centre_x, 370);
        let expected = rgb_of(&data.stripes[0].color);
        for (a, e) in actual.iter().zip(expected) {
            assert!(
                a.abs_diff(e) <= 2,
                "pixel {actual:?}, stripe colour {expected:?}"
            );
        }
        let paper = rgb_of("#0f1115");
        let above = pixel(&image, 360, 172);
        assert_ne!(above, paper, "no glow just above the stripes");
        assert_eq!(pixel(&image, 360, 30), paper, "the glow reaches the header");
    }

    #[test]
    fn the_classic_colours_come_from_the_theme() {
        // Without stripes and legend, whose palette colours include the accent.
        let data = CardData {
            theme: Theme {
                paper: "#010203",
                ink: "#040506",
                muted: "#070809",
                accent: "#0a0b0c",
            },
            stripes: Vec::new(),
            legend: Vec::new(),
            ..fixture("Mira Okafor")
        };
        let svg = Renderer::new().unwrap().svg(&data).unwrap();
        for colour in ["#010203", "#040506", "#0a0b0c"] {
            assert!(svg.contains(colour), "no {colour} in {svg}");
        }
        for literal in ["#f4efe6", "#1b1b1b", "#e8601c"] {
            assert!(!svg.contains(literal), "{literal} in {svg}");
        }
    }

    #[test]
    fn the_context_has_the_variant_theme_split_views_and_rounded_numbers() {
        let data = CardData {
            splits: vec![SplitView {
                index: 43,
                pace: "5:20".into(),
                color: "#e8601c".into(),
                partial: true,
                fraction: 0.195_000_01,
                t: 0.666_666,
            }],
            avg_t: 0.333_333,
            ..fixture_as(Variant::Noir)
        };
        let context = template_context(&data);
        let attr = |value: &Value, name: &str| value.get_attr(name).unwrap();
        assert_eq!(attr(&context, "variant").as_str(), Some("noir"));
        let theme = attr(&context, "theme");
        assert_eq!(attr(&theme, "paper").as_str(), Some("#0f1115"));
        assert_eq!(attr(&theme, "muted").as_str(), Some("#7d8590"));
        assert_eq!(attr(&context, "avg_t").to_string(), "0.33");
        assert_eq!(attr(&context, "avg_pace").as_str(), Some("5:39"));
        let split = attr(&context, "splits").get_item(&Value::from(0)).unwrap();
        assert_eq!(attr(&split, "index").to_string(), "43");
        assert_eq!(attr(&split, "pace").as_str(), Some("5:20"));
        assert_eq!(attr(&split, "color").as_str(), Some("#e8601c"));
        assert!(attr(&split, "partial").is_true());
        assert_eq!(attr(&split, "fraction").to_string(), "0.2");
        assert_eq!(attr(&split, "t").to_string(), "0.67");
        for block in ["rings", "route_hero", "bars", "mosaic"] {
            assert!(attr(&context, block).is_none(), "{block}");
        }
    }

    #[test]
    fn the_shape_block_of_a_variant_reaches_its_template_and_no_other_does() {
        let split = |index: u32, seconds: f64| Split {
            index,
            distance_m: 1000.0,
            seconds,
            partial: false,
        };
        let point = |lat: f64, lon: f64| TrackPoint {
            lat,
            lon,
            time: None,
        };
        let analysis = Analysis {
            distance_m: 2000.0,
            elapsed_s: 630.0,
            splits: vec![split(1, 300.0), split(2, 330.0)],
            fastest: Some(1),
            start: None,
            points: vec![point(52.5, 13.4), point(52.51, 13.41), point(52.5, 13.42)],
        };
        let meta = Meta {
            runner: "Mira Okafor".into(),
            event: "Spreelauf".into(),
            city: None,
            date: time::macros::date!(2026 - 09 - 27),
        };
        let blocks = [
            (Variant::Rings, "rings"),
            (Variant::Route, "route_hero"),
            (Variant::Bars, "bars"),
            (Variant::Mosaic, "mosaic"),
        ];
        for (variant, key) in blocks {
            let data = card::build(
                &analysis,
                &meta,
                &Palette::light(),
                &TextMeasure::new(),
                variant,
            );
            let context = template_context(&data);
            for (_, other) in blocks {
                let block = context.get_attr(other).unwrap();
                assert_eq!(block.is_none(), other != key, "{other} for {variant}");
            }
        }
    }
}
