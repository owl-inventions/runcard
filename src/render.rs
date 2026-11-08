//! Renders the card: fills the SVG template with minijinja, rasterises it with resvg, encodes the PNG.

use std::sync::Arc;

use minijinja::{context, AutoEscape, Environment, Value};

use crate::error::{Error, Result};
use crate::model::CardData;
use crate::route;
use crate::text::Font;

const TEMPLATE_NAME: &str = "card.svg";
const TEMPLATE: &str = include_str!("../templates/card.svg");

/// Template environment and SVG options with the bundled fonts, built once.
pub struct Renderer {
    env: Environment<'static>,
    opts: usvg::Options<'static>,
}

impl Renderer {
    pub fn new() -> Result<Self> {
        let mut env = Environment::new();
        env.set_auto_escape_callback(|_| AutoEscape::Html);
        env.add_template(TEMPLATE_NAME, TEMPLATE)?;

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

    /// The card as SVG markup.
    pub fn svg(&self, data: &CardData) -> Result<String> {
        let template = self.env.get_template(TEMPLATE_NAME)?;
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
/// feature is off.
fn template_context(data: &CardData) -> Value {
    let stripes: Vec<Value> = data
        .stripes
        .iter()
        .map(|s| context! { width => px(s.width), color => &s.color })
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
fn px(v: f32) -> Value {
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
    use crate::model::Stripe;
    use crate::palette::{anchors, pace_position, Palette};

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
        let palette = Palette::new();
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
}
