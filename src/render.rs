//! Renders the card: fills the variant's SVG template, which extends the
//! shared frame, with minijinja, rasterises it with resvg, encodes the PNG.

use std::sync::Arc;

use minijinja::{context, AutoEscape, Environment, ErrorKind, Output, State, Value};

use crate::error::{Error, Result};
use crate::layout::{
    dots::Dots, ledger::Ledger, ribbon::Ribbon, spiral::Spiral, trail::Trail, wave::Wave,
};
use crate::model::{CardData, Fastest, Measure, RoutePreview, Stats, TitleView};
use crate::text::{Face, Role};
use crate::theme::Tokens;
use crate::variant::Variant;

/// The shared frame every variant template extends: title block, legend,
/// route preview, footer and, in the dark theme, the glow under the
/// visualisation.
const FRAME: &str = include_str!("../templates/frame.svg");

/// The file name and the source of the template of `variant`, compiled in.
fn template(variant: Variant) -> (&'static str, &'static str) {
    match variant {
        Variant::Ribbon => ("ribbon.svg", include_str!("../templates/ribbon.svg")),
        Variant::Spiral => ("spiral.svg", include_str!("../templates/spiral.svg")),
        Variant::Trail => ("trail.svg", include_str!("../templates/trail.svg")),
        Variant::Dots => ("dots.svg", include_str!("../templates/dots.svg")),
        Variant::Wave => ("wave.svg", include_str!("../templates/wave.svg")),
        Variant::Ledger => ("ledger.svg", include_str!("../templates/ledger.svg")),
    }
}

/// Template environment and SVG options with the bundled fonts, built once.
pub struct Renderer {
    env: Environment<'static>,
    opts: usvg::Options<'static>,
}

impl Renderer {
    /// Loads the frame, the templates of every variant and the fonts.
    pub fn new() -> Result<Self> {
        let mut env = environment()?;
        for variant in Variant::ALL {
            let (name, source) = template(variant);
            env.add_template(name, source)?;
        }

        let mut fontdb = fontdb::Database::new();
        for face in Face::ALL {
            fontdb.load_font_source(fontdb::Source::Binary(Arc::new(face.data())));
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

/// The template environment with the frame: XML escaping and the `px`
/// filter, which rounds a template's own arithmetic like the context.
fn environment() -> Result<Environment<'static>> {
    let mut env = Environment::new();
    env.set_auto_escape_callback(|_| AutoEscape::Html);
    env.set_formatter(xml_formatter);
    env.add_filter("px", |value: f64| px(value as f32));
    env.add_template("frame.svg", FRAME)?;
    Ok(env)
}

/// The template context: the fields of `CardData` with the numbers rounded,
/// the user-given texts (title, subline, runner) cleared of characters XML
/// forbids, and `font.<role>` holding the SVG attributes of each type role.
/// Built by hand because minijinja's `serde` feature is off; each shape block
/// builds its own value. The templates round their own arithmetic with the
/// `px` filter.
fn template_context(data: &CardData) -> Value {
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
    context! {
        variant => data.variant,
        theme => theme_value(&data.theme),
        font => font_value(),
        title => title_value(&data.title),
        title_size => px(data.title_size),
        runner => xml_chars(&data.runner),
        subline => xml_chars(&data.subline),
        legend => &data.legend,
        route => data.route.as_ref().map(route_value),
        time => &data.time,
        time_size => px(data.time_size),
        stats => stats_value(&data.stats),
        splits => splits,
        avg_t => px(data.avg_t),
        avg_pace => &data.avg_pace,
        ribbon => data.ribbon.as_ref().map(Ribbon::to_value),
        spiral => data.spiral.as_ref().map(Spiral::to_value),
        trail => data.trail.as_ref().map(Trail::to_value),
        dots => data.dots.as_ref().map(Dots::to_value),
        wave => data.wave.as_ref().map(Wave::to_value),
        ledger => data.ledger.as_ref().map(Ledger::to_value),
    }
}

/// `title.head` and `title.tail`, cleared of characters XML forbids.
fn title_value(title: &TitleView) -> Value {
    context! {
        head => xml_chars(&title.head),
        tail => xml_chars(&title.tail),
    }
}

/// `stats.distance` and `stats.pace`, each a `value` and a `unit`,
/// `stats.pace_x` and `stats.fastest` with its `label`, `pace` and `width`.
fn stats_value(stats: &Stats) -> Value {
    let measure = |measure: &Measure| {
        context! {
            value => &measure.value,
            unit => &measure.unit,
        }
    };
    let fastest = |fastest: &Fastest| {
        context! {
            label => &fastest.label,
            pace => &fastest.pace,
            width => px(fastest.width),
        }
    };
    context! {
        distance => measure(&stats.distance),
        pace => measure(&stats.pace),
        pace_x => px(stats.pace_x),
        fastest => stats.fastest.as_ref().map(fastest),
    }
}

/// `route.path` and `route.start.x`, `route.start.y`, the start rounded to
/// two decimals.
fn route_value(route: &RoutePreview) -> Value {
    let (x, y) = route.start;
    context! {
        path => &route.path,
        start => context! { x => px(x), y => px(y) },
    }
}

fn theme_value(theme: &Tokens) -> Value {
    context! {
        paper => theme.paper,
        ink => theme.ink,
        muted => theme.muted,
        rule => theme.rule,
        accent => theme.accent,
        marker => theme.marker,
        on_fast => theme.on_fast,
        dark => theme.dark,
    }
}

/// `font.display`, `font.display_italic`, `font.numeral`, `font.label`,
/// `font.text`, `font.strong` and `font.mono`: the family, style and
/// variation attributes of each role, marked safe so the quotes survive
/// auto-escaping.
fn font_value() -> Value {
    let attributes = |role: Role| Value::from_safe_string(role.attributes());
    context! {
        display => attributes(Role::Display),
        numeral => attributes(Role::Numeral),
        label => attributes(Role::Label),
        display_italic => attributes(Role::DisplayItalic),
        text => attributes(Role::Text),
        strong => attributes(Role::Strong),
        mono => attributes(Role::Mono),
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
/// the `13.300000190734863` of a widened `f32`, and `40`, not `40.0`.
pub(crate) fn px(v: f32) -> Value {
    let rounded = (f64::from(v) * 100.0).round() / 100.0;
    if rounded.fract() == 0.0 {
        Value::from(rounded as i64)
    } else {
        Value::from(rounded)
    }
}

/// Writes template values like minijinja's default formatter, except that a
/// string that is not marked safe is escaped for XML (`&`, `<`, `>`, `"`,
/// `'`), not for HTML, which would also write `/` as `&#x2f;`.
fn xml_formatter(
    out: &mut Output,
    state: &mut State,
    value: &Value,
) -> std::result::Result<(), minijinja::Error> {
    match value.as_str() {
        Some(text) if !value.is_safe() && !matches!(state.auto_escape(), AutoEscape::None) => {
            let escaped = xml_escape(text);
            std::fmt::Write::write_str(out, &escaped)
                .map_err(|_| minijinja::Error::new(ErrorKind::WriteFailure, "formatting failed"))
        }
        _ => minijinja::escape_formatter(out, state, value),
    }
}

/// `text` with the five XML special characters written as entities.
fn xml_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            c => escaped.push(c),
        }
    }
    escaped
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
    use crate::model::{Analysis, Meta, RoutePreview, Split, SplitView, TrackPoint};
    use crate::palette::Palette;
    use crate::text::{Role, TextMeasure};
    use crate::theme::{Theme, Tokens};

    /// The fastest kilometre of the fixture: its pace 36.5 px wide.
    fn fastest() -> Fastest {
        Fastest {
            label: "fastest km 21".into(),
            pace: "4:48".into(),
            width: 36.5,
        }
    }

    fn fixture(runner: &str) -> CardData {
        CardData {
            variant: "ribbon",
            theme: Theme::Light.tokens(),
            title: TitleView {
                head: "Harbour ".into(),
                tail: "Half".into(),
            },
            title_size: 76.0,
            runner: runner.into(),
            subline: "LISBON \u{b7} 19 APRIL 2026".into(),
            legend: Palette::for_theme(Theme::Light).legend(32),
            route: None,
            time: "1:48:58".into(),
            time_size: 132.0,
            stats: Stats {
                distance: Measure {
                    value: "21.10".into(),
                    unit: "km".into(),
                },
                pace: Measure {
                    value: "5:10".into(),
                    unit: "/km".into(),
                },
                pace_x: 163.456,
                fastest: Some(fastest()),
            },
            splits: Vec::new(),
            avg_t: 0.5,
            avg_pace: "5:10".into(),
            ribbon: None,
            spiral: None,
            trail: None,
            dots: None,
            wave: None,
            ledger: None,
        }
    }

    /// The fixture as `variant` in `theme` would get it.
    fn fixture_as(variant: Variant, theme: Theme) -> CardData {
        let palette = Palette::for_theme(theme);
        CardData {
            variant: variant.name(),
            theme: theme.tokens(),
            legend: palette.legend(32),
            ..fixture("Mara Lindqvist")
        }
    }

    /// The fixture without a fastest kilometre.
    fn without_fastest(data: CardData) -> CardData {
        CardData {
            stats: Stats {
                fastest: None,
                ..data.stats.clone()
            },
            ..data
        }
    }

    /// A route preview from (600, 776.26) to the box's right edge.
    fn preview() -> RoutePreview {
        RoutePreview {
            path: "M 600.0 776.3 L 650.5 806.0 L 672.0 750.0".into(),
            start: (600.004, 776.256),
        }
    }

    fn svg_of(data: &CardData) -> String {
        Renderer::new().unwrap().svg(data).unwrap()
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

    fn assert_colour(actual: [u8; 3], hex: &str, what: &str) {
        let expected = rgb_of(hex);
        assert!(
            actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 2),
            "{what} is {actual:?}, expected {hex} {expected:?}"
        );
    }

    /// The start tag of the `<text>` element whose content is `content`.
    fn text_tag<'a>(svg: &'a str, content: &str) -> &'a str {
        let end = svg
            .find(&format!(">{content}</text>"))
            .unwrap_or_else(|| panic!("no text {content:?} in {svg}"));
        let start = svg[..end].rfind("<text").expect("a text start tag");
        &svg[start..=end]
    }

    fn assert_attributes(tag: &str, attributes: &[&str]) {
        for attribute in attributes {
            assert!(tag.contains(attribute), "no {attribute} in {tag}");
        }
    }

    /// The text of every `<text>` element, its tspans included, in order.
    fn texts(svg: &str) -> Vec<String> {
        svg.split("<text")
            .skip(1)
            .map(|element| {
                let content =
                    &element[element.find('>').unwrap() + 1..element.find("</text>").unwrap()];
                let mut text = String::new();
                let mut in_tag = false;
                for c in content.chars() {
                    match c {
                        '<' => in_tag = true,
                        '>' => in_tag = false,
                        c if !in_tag => text.push(c),
                        _ => {}
                    }
                }
                text
            })
            .collect()
    }

    const INK: &str = r##"fill="#17191d""##;
    const MUTED: &str = r##"fill="#8a8d93""##;
    const ACCENT: &str = r##"fill="#e2492a""##;

    #[test]
    fn the_title_sets_its_head_in_display_and_its_last_word_in_italic() {
        let svg = svg_of(&fixture("Mara Lindqvist"));
        let title = format!(
            r#"<text x="46" y="108" {} font-size="76" {INK}>Harbour <tspan {}>Half</tspan></text>"#,
            Role::Display.attributes(),
            Role::DisplayItalic.attributes()
        );
        assert!(svg.contains(&title), "no {title} in {svg}");
        assert!(Role::DisplayItalic
            .attributes()
            .contains(r#"font-style="italic""#));
    }

    #[test]
    fn a_title_without_a_tail_is_one_text_without_a_tspan() {
        let data = CardData {
            title: TitleView {
                head: "Spreelauf".into(),
                tail: String::new(),
            },
            title_size: 51.5,
            ..fixture("Mara Lindqvist")
        };
        let svg = svg_of(&data);
        let title = format!(
            r#"<text x="46" y="108" {} font-size="51.5" {INK}>Spreelauf</text>"#,
            Role::Display.attributes()
        );
        assert!(svg.contains(&title), "no {title} in {svg}");
        assert!(!svg.contains("<tspan></tspan>"), "{svg}");
    }

    #[test]
    fn the_byline_sets_the_runner_in_the_italic_at_26_px() {
        let svg = svg_of(&fixture("Mara Lindqvist"));
        assert_attributes(
            text_tag(&svg, "Mara Lindqvist"),
            &[
                r#"x="48""#,
                r#"y="146""#,
                r#"font-size="26""#,
                INK,
                &Role::DisplayItalic.attributes(),
            ],
        );
    }

    #[test]
    fn the_subline_is_a_muted_label_tracked_by_2_2() {
        let svg = svg_of(&fixture("Mara Lindqvist"));
        assert_attributes(
            text_tag(&svg, "LISBON \u{b7} 19 APRIL 2026"),
            &[
                r#"x="48""#,
                r#"y="174""#,
                r#"font-size="11""#,
                r#"letter-spacing="2.2""#,
                MUTED,
                &Role::Label.attributes(),
            ],
        );
    }

    #[test]
    fn a_one_pixel_rule_spans_the_content_width_at_740() {
        let svg = svg_of(&fixture("Mara Lindqvist"));
        assert!(
            svg.contains(r##"<rect x="48" y="740" width="624" height="1" fill="#d9dad6"/>"##),
            "{svg}"
        );
    }

    #[test]
    fn the_legend_is_a_bar_of_32_five_by_four_rects_between_slower_and_faster() {
        let data = fixture("Mara Lindqvist");
        let svg = svg_of(&data);
        for (i, colour) in data.legend.iter().enumerate() {
            let rect = format!(
                r#"<rect x="{}" y="756" width="5" height="4" fill="{colour}"/>"#,
                48 + 5 * i
            );
            assert!(svg.contains(&rect), "no {rect} in {svg}");
        }
        let caption = [
            r#"y="778""#,
            r#"font-size="10""#,
            r#"letter-spacing="1.5""#,
            MUTED,
            &Role::Label.attributes(),
        ];
        assert_attributes(text_tag(&svg, "SLOWER"), &caption);
        assert_attributes(text_tag(&svg, "SLOWER"), &[r#"x="48""#]);
        assert!(!text_tag(&svg, "SLOWER").contains("text-anchor"));
        // resvg tracks after the last letter too: the end anchor makes up for it.
        assert_attributes(text_tag(&svg, "FASTER"), &caption);
        assert_attributes(
            text_tag(&svg, "FASTER"),
            &[r#"x="209.5""#, r#"text-anchor="end""#],
        );
    }

    #[test]
    fn the_time_is_the_numeral_tracked_by_minus_3_at_its_fitted_size() {
        let svg = svg_of(&fixture("Mara Lindqvist"));
        assert_attributes(
            text_tag(&svg, "1:48:58"),
            &[
                r#"x="44""#,
                r#"y="912""#,
                r#"font-size="132""#,
                r#"letter-spacing="-3""#,
                INK,
                &Role::Numeral.attributes(),
            ],
        );
        let data = CardData {
            time: "11111:06:40".into(),
            time_size: 101.25,
            ..fixture("Mara Lindqvist")
        };
        assert_attributes(
            text_tag(&svg_of(&data), "11111:06:40"),
            &[r#"font-size="101.25""#],
        );
    }

    #[test]
    fn the_stats_line_sets_distance_and_pace_in_ink_their_units_muted() {
        let svg = svg_of(&fixture("Mara Lindqvist"));
        let text = Role::Text.attributes();
        // The space goes before the tspan: resvg drops a tspan's leading space.
        let distance = format!(
            r#"<text x="48" y="954" {text} font-size="17" {INK}>21.10 <tspan {MUTED}>km</tspan></text>"#
        );
        let pace = format!(
            r#"<text x="163.46" y="954" {text} font-size="17" {INK}>5:10 <tspan {MUTED}>/km</tspan></text>"#
        );
        for line in [distance, pace] {
            assert!(svg.contains(&line), "no {line} in {svg}");
        }
    }

    #[test]
    fn the_fastest_pace_ends_the_stats_line_in_strong_accent_after_its_label() {
        let svg = svg_of(&fixture("Mara Lindqvist"));
        let pace = format!(
            r#"<text x="672" y="954" text-anchor="end" {} font-size="17" {ACCENT}>4:48</text>"#,
            Role::Strong.attributes()
        );
        let label = format!(
            r#"<text x="621.5" y="954" text-anchor="end" {} font-size="17" {MUTED}>fastest km 21</text>"#,
            Role::Text.attributes()
        );
        assert_eq!(svg.matches(&pace).count(), 1, "no {pace} in {svg}");
        assert!(svg.contains(&label), "no {label} in {svg}");
    }

    #[test]
    fn on_light_paper_a_marker_pen_stroke_lies_under_the_fastest_pace() {
        let svg = svg_of(&fixture("Mara Lindqvist"));
        // 672 - 36.5 - 7, 36.5 + 14 wide, turned about its centre 672 - 36.5 / 2.
        let mark = format!(
            r#"<rect x="628.5" y="936" width="50.5" height="25" rx="3" {ACCENT} opacity="0.18" transform="rotate(-1.5 653.75 948.5)"/>"#
        );
        let at = svg
            .find(&mark)
            .unwrap_or_else(|| panic!("no {mark} in {svg}"));
        assert!(
            at < svg.find(">4:48</text>").unwrap(),
            "the mark is drawn over the pace"
        );
        assert!(!svg.contains("textglow"), "{svg}");
        assert!(!svg.contains("filter"), "{svg}");
    }

    #[test]
    fn on_dark_paper_a_blurred_copy_glows_under_the_fastest_pace() {
        let svg = svg_of(&fixture_as(Variant::Ribbon, Theme::Dark));
        let defs = &svg[svg.find("<defs>").expect("defs")..svg.find("</defs>").expect("defs")];
        let filter = &defs[defs
            .find(r#"<filter id="textglow" x="-50%" y="-50%" width="200%" height="200%">"#)
            .unwrap_or_else(|| panic!("no textglow filter in {defs}"))..];
        let filter = &filter[..filter.find("</filter>").expect("a closed filter")];
        assert!(
            filter.contains(r#"<feGaussianBlur stdDeviation="5"/>"#),
            "{filter}"
        );
        let pace = format!(
            r##"<text x="672" y="954" text-anchor="end" {} font-size="17" fill="#ff5a36">4:48</text>"##,
            Role::Strong.attributes()
        );
        let glow = format!(r#"<g filter="url(#textglow)" opacity="0.9">{pace}</g>"#);
        let at = svg
            .find(&glow)
            .unwrap_or_else(|| panic!("no {glow} in {svg}"));
        assert_eq!(svg.matches(&pace).count(), 2, "{svg}");
        assert!(
            svg.rfind(&pace).unwrap() > at,
            "the crisp pace is not on top"
        );
        assert!(
            !svg.contains(r#"opacity="0.18""#),
            "a marker rect on dark: {svg}"
        );
    }

    #[test]
    fn without_a_fastest_kilometre_the_fastest_block_is_left_out() {
        for theme in Theme::ALL {
            let svg = svg_of(&without_fastest(fixture_as(Variant::Ribbon, theme)));
            assert!(!svg.contains("fastest km"), "{theme}: {svg}");
            assert!(!svg.contains(r#"opacity="0.18""#), "{theme}: {svg}");
            assert!(!svg.contains("url(#textglow)"), "{theme}: {svg}");
            assert!(
                texts(&svg).iter().all(|text| !text.is_empty()),
                "{theme}: an empty text in {svg}"
            );
            assert!(svg.contains(">/km</tspan></text>"), "{theme}: {svg}");
        }
    }

    #[test]
    fn the_frame_draws_no_other_text() {
        let light = svg_of(&fixture("Mara Lindqvist"));
        let expected = [
            "Harbour Half",
            "Mara Lindqvist",
            "LISBON \u{b7} 19 APRIL 2026",
            "SLOWER",
            "FASTER",
            "1:48:58",
            "21.10 km",
            "5:10 /km",
            "fastest km 21",
            "4:48",
        ];
        assert_eq!(texts(&light), expected);
        let dark = svg_of(&fixture_as(Variant::Ribbon, Theme::Dark));
        // The blurred copy of the fastest pace, then the crisp one.
        let mut glowing = expected.to_vec();
        glowing.insert(9, "4:48");
        assert_eq!(texts(&dark), glowing);
    }

    #[test]
    fn the_frame_draws_the_header_the_picture_and_the_footer_in_that_order() {
        let mut env = environment().unwrap();
        env.add_template(
            "probe.svg",
            r#"{% extends "frame.svg" %}{% block viz %}<rect id="probe"/>{% endblock %}"#,
        )
        .unwrap();
        let svg = env
            .get_template("probe.svg")
            .unwrap()
            .render(template_context(&fixture("Mara Lindqvist")))
            .unwrap();
        let at = |what: &str| {
            svg.find(what)
                .unwrap_or_else(|| panic!("no {what} in {svg}"))
        };
        let order = [
            at(r#"<rect x="0" y="0" width="720" height="1000""#),
            at(">Harbour <tspan"),
            at(">Mara Lindqvist</text>"),
            at(">LISBON"),
            at(r#"<rect id="probe"/>"#),
            at(r#"y="740""#),
            at(">SLOWER</text>"),
            at(">1:48:58</text>"),
            at(">21.10 <tspan"),
            at(">4:48</text>"),
        ];
        assert!(order.is_sorted(), "{order:?} in {svg}");
    }

    #[test]
    fn the_px_filter_rounds_the_template_arithmetic_like_the_context() {
        let mut env = environment().unwrap();
        env.add_template(
            "probe.svg",
            "{{ (672 - width - 14) | px }} {{ (672 - width / 2) | px }} {{ 40.0 | px }} {{ 7 | px }}",
        )
        .unwrap();
        let out = env
            .get_template("probe.svg")
            .unwrap()
            .render(context! { width => px(33.456) })
            .unwrap();
        assert_eq!(out, "624.54 655.27 40 7");
    }

    #[test]
    fn the_svg_escapes_markup_in_the_names_as_xml() {
        let svg = svg_of(&fixture("Tom & Jerry"));
        assert!(svg.contains(">Tom &amp; Jerry</text>"), "{svg}");
        assert!(!svg.contains("Tom & Jerry"), "{svg}");
        let svg = svg_of(&fixture(r#"<b>"Jo" O'Neil</b>"#));
        assert!(
            svg.contains(">&lt;b&gt;&quot;Jo&quot; O&apos;Neil&lt;/b&gt;</text>"),
            "{svg}"
        );
        assert!(svg.contains(">/km</tspan>"), "a slash is escaped in {svg}");
        let data = CardData {
            title: TitleView {
                head: "Fish & ".into(),
                tail: "<Chips>".into(),
            },
            ..fixture("Mara Lindqvist")
        };
        let svg = svg_of(&data);
        assert!(svg.contains(">Fish &amp; <tspan"), "{svg}");
        assert!(svg.contains(">&lt;Chips&gt;</tspan>"), "{svg}");
    }

    #[test]
    fn px_rounds_to_two_decimals_and_writes_whole_numbers_without_a_point() {
        assert_eq!(px(13.3).to_string(), "13.3");
        assert_eq!(px(31.456).to_string(), "31.46");
        assert_eq!(px(40.0).to_string(), "40");
        assert_eq!(px(-2.0).to_string(), "-2");
        assert_eq!(px(0.004).to_string(), "0");
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
            subline: "LIS\u{1b}BON \u{b7} 19 APRIL 2026".into(),
            title: TitleView {
                head: "Harbour\u{b} ".into(),
                tail: "Half\u{fffe}".into(),
            },
            ..fixture("Mara\u{1} Lindqvist")
        };
        let renderer = Renderer::new().unwrap();
        let svg = renderer.svg(&data).unwrap();
        assert!(svg.contains(">Mara Lindqvist</text>"), "{svg}");
        assert!(svg.contains(">Harbour <tspan"), "{svg}");
        assert!(svg.contains(">Half</tspan>"), "{svg}");
        assert!(svg.contains(">LISBON \u{b7} 19 APRIL 2026</text>"), "{svg}");
        renderer.png(&svg, 1.0).expect("the svg rasterises");
    }

    #[test]
    fn the_png_has_the_card_size_times_the_scale() {
        let renderer = Renderer::new().unwrap();
        let data = fixture("Mara Lindqvist");
        let (w, h, _) = decode(&renderer.render(&data, 1.0).unwrap());
        assert_eq!((w, h), (720, 1000));
        let (w, h, _) = decode(&renderer.render(&data, 2.0).unwrap());
        assert_eq!((w, h), (1440, 2000));
    }

    #[test]
    fn the_png_shows_the_paper_the_legend_ends_and_the_rule_in_both_themes() {
        let renderer = Renderer::new().unwrap();
        for theme in Theme::ALL {
            let data = fixture_as(Variant::Ribbon, theme);
            let image = decode(&renderer.render(&data, 1.0).unwrap());
            let tokens = theme.tokens();
            assert_colour(pixel(&image, 5, 5), tokens.paper, &format!("{theme} paper"));
            assert_colour(
                pixel(&image, 50, 758),
                &data.legend[0],
                &format!("{theme} slower end"),
            );
            assert_colour(
                pixel(&image, 205, 758),
                &data.legend[31],
                &format!("{theme} faster end"),
            );
            assert_colour(
                pixel(&image, 360, 740),
                tokens.rule,
                &format!("{theme} rule"),
            );
        }
    }

    #[test]
    fn the_distance_inks_as_wide_as_its_figure_a_space_and_its_unit() {
        let scale = 4.0;
        let data = without_fastest(fixture("Mara Lindqvist"));
        let image = decode(&Renderer::new().unwrap().render(&data, scale).unwrap());
        let paper = rgb_of(Theme::Light.tokens().paper);
        let px = |card: f32| (card * scale) as u32;
        let inked: Vec<u32> = (px(40.0)..px(data.stats.pace_x - 10.0))
            .filter(|&x| (px(940.0)..px(958.0)).any(|y| pixel(&image, x, y) != paper))
            .collect();
        let width = (inked[inked.len() - 1] - inked[0] + 1) as f32 / scale;
        let measured = TextMeasure::new().width(Role::Text, "21.10 km", 17.0);
        // The side bearings of the 2 and the m stay blank; without its 4.3 px
        // space the line would ink about 6 px short.
        assert!(
            (measured - 3.0..=measured + 1.0).contains(&width),
            "the distance inks {width} px, measured {measured} px"
        );
    }

    #[test]
    fn the_png_tints_the_light_marker_pen_stroke_with_the_accent() {
        let renderer = Renderer::new().unwrap();
        let image = decode(&renderer.render(&fixture("Mara Lindqvist"), 1.0).unwrap());
        // Left of the pace, inside the stroke: 18% accent over the paper.
        let tokens = Theme::Light.tokens();
        let (paper, accent) = (rgb_of(tokens.paper), rgb_of(tokens.accent));
        let tint: Vec<String> = (0..3)
            .map(|i| {
                let c = 0.82 * f32::from(paper[i]) + 0.18 * f32::from(accent[i]);
                format!("{:02x}", c.round() as u8)
            })
            .collect();
        assert_colour(
            pixel(&image, 631, 948),
            &format!("#{}", tint.concat()),
            "the stroke",
        );
        let plain = decode(
            &renderer
                .render(&without_fastest(fixture("Mara Lindqvist")), 1.0)
                .unwrap(),
        );
        assert_colour(
            pixel(&plain, 631, 948),
            Theme::Light.tokens().paper,
            "no stroke",
        );
    }

    #[test]
    fn the_png_is_compressed() {
        let png = Renderer::new()
            .unwrap()
            .render(&fixture("Mara Lindqvist"), 1.0)
            .unwrap();
        let raw_rgb = 720 * 1000 * 3;
        assert!(png.len() < raw_rgb / 4, "{} bytes", png.len());
    }

    #[test]
    fn unparsable_svg_is_an_svg_error() {
        let error = Renderer::new().unwrap().png("<svg", 1.0).unwrap_err();
        assert!(matches!(error, crate::Error::Svg(_)), "{error:?}");
    }

    #[test]
    fn every_variant_renders_its_template_inside_the_frame_in_both_themes() {
        let renderer = Renderer::new().unwrap();
        for theme in Theme::ALL {
            for variant in Variant::ALL {
                let data = fixture_as(variant, theme);
                let svg = renderer.svg(&data).unwrap();
                let what = format!("{variant} {theme}");
                assert!(svg.starts_with("<svg"), "{what}: {svg}");
                assert!(svg.trim_end().ends_with("</svg>"), "{what}: {svg}");
                assert!(!svg.contains("{{") && !svg.contains("{%"), "{what}: {svg}");
                assert!(svg.contains(">Mara Lindqvist</text>"), "{what}: {svg}");
                assert!(svg.contains(">1:48:58</text>"), "{what}: {svg}");
                let paper = format!(r#"fill="{}""#, data.theme.paper);
                assert!(svg.contains(&paper), "{what} has no {paper}: {svg}");
                let image = decode(&renderer.png(&svg, 1.0).unwrap());
                assert_eq!((image.0, image.1), (720, 1000), "{what}");
                assert_eq!(pixel(&image, 2, 2), rgb_of(data.theme.paper), "{what}");
            }
        }
    }

    #[test]
    fn the_dark_theme_draws_a_blurred_copy_of_the_visualisation_under_it() {
        let renderer = Renderer::new().unwrap();
        for variant in Variant::ALL {
            let dark = renderer.svg(&fixture_as(variant, Theme::Dark)).unwrap();
            assert!(
                dark.contains(r#"<filter id="glow" x="-15%" y="-30%" width="130%" height="160%">"#),
                "{variant}: {dark}"
            );
            assert!(
                dark.contains(r#"<feGaussianBlur stdDeviation="14"/>"#),
                "{variant}: {dark}"
            );
            assert_eq!(
                dark.matches(r#"<g filter="url(#glow)" opacity="0.35">"#)
                    .count(),
                1,
                "{variant}: {dark}"
            );
            let light = renderer.svg(&fixture_as(variant, Theme::Light)).unwrap();
            assert!(!light.contains("feGaussianBlur"), "{variant}: {light}");
            assert!(!light.contains("filter"), "{variant}: {light}");
            for svg in [dark, light] {
                let image = decode(&renderer.png(&svg, 1.0).unwrap());
                assert_eq!((image.0, image.1), (720, 1000), "{variant}");
            }
        }
    }

    #[test]
    fn the_glow_repeats_the_visualisation_block_exactly_and_the_light_theme_draws_it_once() {
        let mut env = environment().unwrap();
        env.add_template(
            "probe.svg",
            r#"{% extends "frame.svg" %}{% block viz %}<rect id="probe" fill="{{ theme.ink }}"/>{% endblock %}"#,
        )
        .unwrap();
        let probe = r##"<rect id="probe" fill="#f4efe6"/>"##;
        let dark = env
            .get_template("probe.svg")
            .unwrap()
            .render(template_context(&fixture_as(Variant::Ribbon, Theme::Dark)))
            .unwrap();
        assert_eq!(dark.matches(probe).count(), 2, "{dark}");
        let glow = dark
            .split(r#"<g filter="url(#glow)" opacity="0.35">"#)
            .nth(1)
            .and_then(|rest| rest.split("</g>").next())
            .unwrap_or_else(|| panic!("no glow group in {dark}"));
        assert!(glow.contains(probe), "{glow}");
        let light = env
            .get_template("probe.svg")
            .unwrap()
            .render(template_context(&fixture_as(Variant::Ribbon, Theme::Light)))
            .unwrap();
        assert_eq!(
            light
                .matches(r##"<rect id="probe" fill="#17191d"/>"##)
                .count(),
            1,
            "{light}"
        );
    }

    /// Every `#rrggbb` literal in `svg`.
    fn colour_literals(svg: &str) -> Vec<&str> {
        svg.match_indices('#')
            .map(|(i, _)| &svg[i..(i + 7).min(svg.len())])
            .filter(|c| c.len() == 7 && c[1..].chars().all(|h| h.is_ascii_hexdigit()))
            .collect()
    }

    #[test]
    fn the_frame_takes_every_colour_from_the_theme_and_the_legend() {
        let renderer = Renderer::new().unwrap();
        for (variant, dark) in Variant::ALL
            .into_iter()
            .zip([false, true].into_iter().cycle())
        {
            let tokens = Tokens {
                paper: "#010203",
                ink: "#040506",
                muted: "#070809",
                rule: "#0a0b0c",
                accent: "#101112",
                marker: "#131415",
                on_fast: "#161718",
                dark,
            };
            let data = CardData {
                theme: tokens,
                legend: vec!["#0d0e0f".into(); 32],
                route: Some(preview()),
                ..fixture_as(variant, Theme::Light)
            };
            let svg = renderer.svg(&data).unwrap();
            let mut colours = colour_literals(&svg);
            colours.sort_unstable();
            colours.dedup();
            assert_eq!(
                colours,
                ["#010203", "#040506", "#070809", "#0a0b0c", "#0d0e0f", "#101112"],
                "{variant}"
            );
        }
    }

    #[test]
    fn the_frame_draws_the_route_preview_after_the_legend_once_in_both_themes() {
        for theme in Theme::ALL {
            let data = CardData {
                route: Some(preview()),
                ..fixture_as(Variant::Ribbon, theme)
            };
            let svg = svg_of(&data);
            let ink = theme.tokens().ink;
            let path = format!(
                r#"<path d="M 600.0 776.3 L 650.5 806.0 L 672.0 750.0" fill="none" stroke="{ink}" stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round"/>"#
            );
            let start = format!(r#"<circle cx="600" cy="776.26" r="2.5" fill="{ink}"/>"#);
            for tag in [&path, &start] {
                assert_eq!(
                    svg.matches(tag.as_str()).count(),
                    1,
                    "{theme}: {tag} in {svg}"
                );
                let at = svg.find(tag.as_str()).unwrap();
                assert!(
                    at > svg.find(">FASTER</text>").unwrap(),
                    "{theme}: {tag} before the legend"
                );
                assert!(
                    at < svg.find(">1:48:58</text>").unwrap(),
                    "{theme}: {tag} after the time"
                );
            }
            let without = svg_of(&fixture_as(Variant::Ribbon, theme));
            assert!(
                !without.contains(r#"stroke-width="1.5""#),
                "{theme}: {without}"
            );
            assert!(!without.contains(r#"r="2.5""#), "{theme}: {without}");
        }
    }

    #[test]
    fn the_context_has_the_frame_the_theme_the_roles_and_rounded_numbers() {
        let data = CardData {
            splits: vec![SplitView {
                index: 22,
                pace: "4:50".into(),
                color: "#efd35a".into(),
                partial: true,
                fraction: 0.100_000_01,
                t: 0.666_666,
            }],
            avg_t: 0.333_333,
            title_size: 31.456,
            time_size: 101.234,
            ..fixture_as(Variant::Wave, Theme::Dark)
        };
        let context = template_context(&data);
        let attr = |value: &Value, name: &str| value.get_attr(name).unwrap();
        assert_eq!(attr(&context, "variant").as_str(), Some("wave"));
        let theme = attr(&context, "theme");
        for (key, colour) in [
            ("paper", "#0f1115"),
            ("ink", "#f4efe6"),
            ("muted", "#7d8590"),
            ("rule", "#262b33"),
            ("accent", "#ff5a36"),
            ("marker", "#f4efe6"),
            ("on_fast", "#0f1115"),
        ] {
            assert_eq!(attr(&theme, key).as_str(), Some(colour), "theme.{key}");
        }
        assert!(attr(&theme, "dark").is_true());
        let font = attr(&context, "font");
        for (key, role) in [
            ("display", Role::Display),
            ("display_italic", Role::DisplayItalic),
            ("numeral", Role::Numeral),
            ("label", Role::Label),
            ("text", Role::Text),
            ("strong", Role::Strong),
            ("mono", Role::Mono),
        ] {
            let value = attr(&font, key);
            assert!(value.is_safe(), "font.{key} would be escaped");
            assert_eq!(
                value.as_str(),
                Some(role.attributes().as_str()),
                "font.{key}"
            );
        }
        let title = attr(&context, "title");
        assert_eq!(attr(&title, "head").as_str(), Some("Harbour "));
        assert_eq!(attr(&title, "tail").as_str(), Some("Half"));
        for (key, expected) in [
            ("subline", "LISBON \u{b7} 19 APRIL 2026"),
            ("time", "1:48:58"),
            ("runner", "Mara Lindqvist"),
            ("avg_pace", "5:10"),
        ] {
            assert_eq!(attr(&context, key).as_str(), Some(expected), "{key}");
        }
        let stats = attr(&context, "stats");
        let distance = attr(&stats, "distance");
        assert_eq!(attr(&distance, "value").as_str(), Some("21.10"));
        assert_eq!(attr(&distance, "unit").as_str(), Some("km"));
        let pace = attr(&stats, "pace");
        assert_eq!(attr(&pace, "value").as_str(), Some("5:10"));
        assert_eq!(attr(&pace, "unit").as_str(), Some("/km"));
        assert_eq!(attr(&stats, "pace_x").to_string(), "163.46");
        let fastest = attr(&stats, "fastest");
        assert_eq!(attr(&fastest, "label").as_str(), Some("fastest km 21"));
        assert_eq!(attr(&fastest, "pace").as_str(), Some("4:48"));
        assert_eq!(attr(&fastest, "width").to_string(), "36.5");
        let without = template_context(&without_fastest(data.clone()));
        assert!(attr(&attr(&without, "stats"), "fastest").is_none());
        assert_eq!(attr(&context, "title_size").to_string(), "31.46");
        assert_eq!(attr(&context, "time_size").to_string(), "101.23");
        assert_eq!(attr(&context, "avg_t").to_string(), "0.33");
        assert_eq!(attr(&context, "legend").len(), Some(32));
        let split = attr(&context, "splits").get_item(&Value::from(0)).unwrap();
        assert_eq!(attr(&split, "index").to_string(), "22");
        assert_eq!(attr(&split, "pace").as_str(), Some("4:50"));
        assert_eq!(attr(&split, "color").as_str(), Some("#efd35a"));
        assert!(attr(&split, "partial").is_true());
        assert_eq!(attr(&split, "fraction").to_string(), "0.1");
        assert_eq!(attr(&split, "t").to_string(), "0.67");
        for block in [
            "route", "ribbon", "spiral", "trail", "dots", "wave", "ledger",
        ] {
            assert!(attr(&context, block).is_none(), "{block}");
        }
    }

    #[test]
    fn the_route_preview_reaches_the_context_with_its_start_rounded() {
        let data = CardData {
            route: Some(preview()),
            ..fixture_as(Variant::Ribbon, Theme::Light)
        };
        let route = template_context(&data).get_attr("route").unwrap();
        let attr = |value: &Value, name: &str| value.get_attr(name).unwrap();
        assert_eq!(
            attr(&route, "path").as_str(),
            Some("M 600.0 776.3 L 650.5 806.0 L 672.0 750.0")
        );
        let start = attr(&route, "start");
        assert_eq!(attr(&start, "x").to_string(), "600");
        assert_eq!(attr(&start, "y").to_string(), "776.26");
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
            points: vec![point(38.7, -9.15), point(38.71, -9.14), point(38.7, -9.13)],
        };
        let meta = Meta {
            runner: "Mara Lindqvist".into(),
            event: "Harbour Half".into(),
            city: None,
            date: time::macros::date!(2026 - 04 - 19),
        };
        for variant in Variant::ALL {
            let data = card::build(
                &analysis,
                &meta,
                &Palette::for_theme(Theme::Light),
                &TextMeasure::new(),
                variant,
                Theme::Light,
            );
            let context = template_context(&data);
            for other in Variant::ALL {
                let block = context.get_attr(other.name()).unwrap();
                assert_eq!(block.is_none(), other != variant, "{other} for {variant}");
            }
        }
    }
}
