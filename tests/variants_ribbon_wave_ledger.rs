//! The `ribbon`, `wave` and `ledger` variants end to end: GPX in, SVG and PNG
//! out, in both themes.

mod common;

use runcard::pipeline::{CardRequest, Pipeline};
use runcard::{gpx, splits, Theme, Variant};
use time::macros::datetime;
use time::OffsetDateTime;

const START: OffsetDateTime = datetime!(2026-04-19 07:30:00 UTC);

/// Ten kilometres between 4:50 and 5:30, km 7 the fastest at 4:30, then
/// 460 m at 5:00: eleven splits.
fn ten_k() -> String {
    common::synthetic_gpx(
        10_460.0,
        common::paces(&[
            300.0, 310.0, 295.0, 320.0, 305.0, 315.0, 270.0, 300.0, 330.0, 290.0, 300.0,
        ]),
        START,
    )
}

/// A half marathon at 5:10 with a climb over km 13 to 15 and a fast finish:
/// 21 kilometres and 100 m, twenty-two splits.
fn half() -> String {
    let pace = |km: u32| match km {
        13..=15 => 345.0,
        20.. => 290.0,
        _ => 310.0,
    };
    common::synthetic_gpx(21_100.0, pace, START)
}

/// A marathon at paces from 4:50 to 6:00: forty-three splits.
fn marathon() -> String {
    common::synthetic_gpx(42_195.0, |km| 290.0 + f64::from(km * 11 % 15) * 5.0, START)
}

/// 800 m at 5:00: a single partial split.
fn short() -> String {
    common::synthetic_gpx(800.0, |_| 300.0, START)
}

fn split_count(xml: &str) -> usize {
    splits::analyze(&gpx::parse(xml.as_bytes()).unwrap())
        .unwrap()
        .splits
        .len()
}

fn request(variant: Variant, theme: Theme) -> CardRequest {
    CardRequest {
        runner: "Mara Lindqvist".into(),
        event: Some("Harbour Half".into()),
        city: Some("Lisbon".into()),
        date: None,
        scale: 1.0,
        variant,
        theme,
    }
}

fn svg_of(xml: &str, variant: Variant, theme: Theme) -> String {
    Pipeline::new()
        .unwrap()
        .svg(xml.as_bytes(), &request(variant, theme))
        .unwrap()
}

/// How often the visualisation is drawn: once on light paper, twice on dark
/// (the blurred glow under the crisp shapes).
fn copies(theme: Theme) -> usize {
    match theme {
        Theme::Light => 1,
        Theme::Dark => 2,
    }
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

fn rgb_of(hex: &str) -> [u8; 3] {
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
    [channel(1), channel(3), channel(5)]
}

/// The ribbon's bars: rounded rects from x 88 (the legend has a plain one there).
fn bars(svg: &str) -> usize {
    svg.split("<rect ")
        .filter(|tag| tag.starts_with(r#"x="88" "#) && tag.contains(" rx="))
        .count()
}

/// The ledger's paces: the right-aligned mono texts.
fn paces(svg: &str) -> Vec<&str> {
    svg.split("<text ")
        .filter(|tag| tag.contains(r#"text-anchor="end" font-family="JetBrains Mono""#))
        .collect()
}

/// The ledger's swatches: 10 x 10 rects rounded by 3 px.
fn swatches(svg: &str) -> Vec<&str> {
    svg.split("<rect ")
        .filter(|tag| tag.contains(r#"width="10" height="10" rx="3""#))
        .collect()
}

/// The colour a tag is filled with.
fn fill(tag: &str) -> &str {
    let rest = &tag[tag.find(r#"fill=""#).expect("a fill") + 6..];
    &rest[..rest.find('"').unwrap()]
}

const OURS: [Variant; 3] = [Variant::Ribbon, Variant::Wave, Variant::Ledger];

#[test]
fn each_variant_renders_in_light_and_dark_at_the_card_size_without_template_syntax_left() {
    let pipeline = Pipeline::new().unwrap();
    let xml = half();
    for variant in OURS {
        for theme in Theme::ALL {
            let what = format!("{variant} {theme}");
            let rendered = pipeline
                .render(xml.as_bytes(), &request(variant, theme))
                .unwrap();
            let svg = &rendered.svg;
            assert!(svg.starts_with("<svg"), "{what}: {svg}");
            assert!(!svg.contains("{{") && !svg.contains("{%"), "{what}: {svg}");
            // Only the frame's glow filter: no id inside the doubled picture.
            assert_eq!(svg.matches(" id=").count(), copies(theme) - 1, "{what}");
            assert_eq!(
                svg.contains("feGaussianBlur"),
                theme == Theme::Dark,
                "{what}: the glow"
            );
            let (width, height, _) = decode(&rendered.png);
            assert_eq!((width, height), (720, 1000), "{what}");
        }
    }
}

#[test]
fn ribbon_draws_a_bar_and_a_number_per_kilometre_and_outlines_the_fastest() {
    let xml = ten_k();
    assert_eq!(split_count(&xml), 11);
    for theme in Theme::ALL {
        let svg = svg_of(&xml, Variant::Ribbon, theme);
        let copies = copies(theme);
        assert_eq!(bars(&svg), 11 * copies, "{theme}");
        assert_eq!(
            svg.matches(r#"<text x="76" "#).count(),
            11 * copies,
            "{theme}"
        );
        assert_eq!(svg.matches(r#"opacity="0.6""#).count(), copies, "{theme}");
        let outline = format!(
            r#"fill="none" stroke="{}" stroke-width="2""#,
            theme.tokens().ink
        );
        assert_eq!(svg.matches(&outline).count(), copies, "{theme}: {svg}");
        // Ten kilometres of 18 px rows and 4 px gaps from the top of the zone.
        assert!(svg.contains(r#"y="160" width="#), "{svg}");
        assert!(svg.contains(r#"y="380" width="#), "{svg}");
        assert!(svg.contains(r#">11</text>"#), "{svg}");
    }
}

#[test]
fn ribbon_numbers_only_every_fifth_and_the_last_kilometre_of_a_marathon() {
    let svg = svg_of(&marathon(), Variant::Ribbon, Theme::Light);
    assert_eq!(bars(&svg), 43);
    assert_eq!(svg.matches(r#"<text x="76" "#).count(), 9, "{svg}");
    for km in [5, 10, 40, 43] {
        assert!(svg.contains(&format!(">{km}</text>")), "km {km}: {svg}");
    }
}

#[test]
fn wave_draws_a_section_into_every_kilometre_after_the_first_over_its_fill() {
    let xml = ten_k();
    for theme in Theme::ALL {
        let svg = svg_of(&xml, Variant::Wave, theme);
        let copies = copies(theme);
        // Eleven points, ten sections between them, and the fill.
        let sections = r#"stroke-width="4" stroke-linecap="round""#;
        assert_eq!(svg.matches(sections).count(), 10 * copies, "{theme}: {svg}");
        assert_eq!(svg.matches("<path ").count(), 11 * copies, "{theme}");
        let fill = format!(r#"fill="{}" opacity="0.06""#, theme.tokens().ink);
        assert_eq!(svg.matches(&fill).count(), copies, "{theme}: {svg}");
        assert_eq!(svg.matches(r#" 700Z""#).count(), copies, "{theme}: {svg}");
        assert_eq!(
            svg.matches(r#"stroke-dasharray="4 4""#).count(),
            copies,
            "{theme}"
        );
        assert_eq!(svg.matches(">average</text>").count(), copies, "{theme}");
        assert_eq!(svg.matches(r#" r="5""#).count(), copies, "{theme}");
    }
}

#[test]
fn ledger_sets_a_row_per_kilometre_its_fastest_pace_bold() {
    let xml = ten_k();
    for theme in Theme::ALL {
        let svg = svg_of(&xml, Variant::Ledger, theme);
        let copies = copies(theme);
        let paces = paces(&svg);
        assert_eq!(paces.len(), 11 * copies, "{theme}: {svg}");
        // Every pace in ink; the kilometre colours only in the swatches.
        let ink = theme.tokens().ink;
        assert!(paces.iter().all(|pace| fill(pace) == ink), "{theme}: {svg}");
        let swatches = swatches(&svg);
        assert_eq!(swatches.len(), 11 * copies, "{theme}: {svg}");
        for pace in &paces {
            assert!(
                swatches.iter().all(|swatch| fill(swatch) != fill(pace)),
                "{theme}: a pace in a kilometre colour: {pace}"
            );
        }
        let faded: Vec<_> = swatches
            .iter()
            .filter(|swatch| swatch.contains(r#"opacity="0.6""#))
            .collect();
        assert_eq!(faded.len(), copies, "{theme}: the partial kilometre");
        assert_eq!(
            svg.matches(r#"font-variation-settings="'wght' 700""#)
                .count(),
            copies,
            "{theme}"
        );
        assert_eq!(svg.matches(">4:30</text>").count(), copies, "{theme}");
        assert_eq!(svg.matches(">0.46 km</text>").count(), copies, "{theme}");
        // One column: a rule under every row but the last, and the footer's.
        let rules = format!(r#"height="1" fill="{}""#, theme.tokens().rule);
        assert_eq!(svg.matches(&rules).count(), 10 * copies + 1, "{theme}");
    }
}

#[test]
fn ledger_spreads_a_half_over_two_columns() {
    let xml = half();
    assert_eq!(split_count(&xml), 22);
    let svg = svg_of(&xml, Variant::Ledger, Theme::Light);
    // Eleven rows in each column: the first from x 48 to 348, the second
    // from x 372 to 672.
    let numbers = format!(
        r#"{} font-size="13" fill="{}""#,
        runcard::text::Role::Mono.attributes(),
        Theme::Light.tokens().muted
    );
    assert_eq!(svg.matches(&numbers).count(), 22, "{svg}");
    assert_eq!(svg.matches(r#"<text x="372" "#).count(), 11, "{svg}");
    assert_eq!(svg.matches(r#"<text x="348" "#).count(), 11, "{svg}");
    assert_eq!(swatches(&svg).len(), 22, "{svg}");
    let rules = format!(r#"height="1" fill="{}""#, Theme::Light.tokens().rule);
    assert_eq!(svg.matches(&rules).count(), 20 + 1);
}

#[test]
fn a_single_split_is_one_bar_one_level_line_and_one_row() {
    let xml = short();
    assert_eq!(split_count(&xml), 1);
    let ribbon = svg_of(&xml, Variant::Ribbon, Theme::Light);
    assert_eq!(bars(&ribbon), 1, "{ribbon}");
    assert!(!ribbon.contains(r#"fill="none""#), "{ribbon}");

    let wave = svg_of(&xml, Variant::Wave, Theme::Light);
    assert_eq!(
        wave.matches(r#"stroke-linecap="round""#).count(),
        1,
        "{wave}"
    );
    assert!(
        wave.contains(r#" 670 "#),
        "the level line's cap reaches x 672: {wave}"
    );
    assert!(wave.contains(r#"d="M50 "#), "{wave}");
    assert!(!wave.contains(r#" r="5""#), "no fastest kilometre: {wave}");

    let ledger = svg_of(&xml, Variant::Ledger, Theme::Light);
    let paces = r#"text-anchor="end" font-family="JetBrains Mono""#;
    assert_eq!(ledger.matches(paces).count(), 1, "{ledger}");
    assert!(!ledger.contains("'wght' 700"), "{ledger}");
    assert!(ledger.contains(">0.80 km</text>"), "{ledger}");
}

#[test]
fn on_light_paper_nothing_is_drawn_between_the_title_block_the_zone_and_the_legend() {
    let pipeline = Pipeline::new().unwrap();
    let paper = rgb_of(Theme::Light.tokens().paper);
    for xml in [ten_k(), half(), marathon()] {
        for variant in OURS {
            let png = pipeline
                .png(xml.as_bytes(), &request(variant, Theme::Light))
                .unwrap();
            let (width, _, rgb) = decode(&png);
            let pixel = |x: u32, y: u32| {
                let i = ((y * width + x) * 3) as usize;
                [rgb[i], rgb[i + 1], rgb[i + 2]]
            };
            // Under the subline down to the zone, and under the zone down to
            // the legend.
            for y in (124..158).chain(722..756) {
                for x in 0..720 {
                    assert_eq!(pixel(x, y), paper, "{variant}: ink at ({x}, {y})");
                }
            }
        }
    }
}
