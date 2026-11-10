//! The bundled fonts: real font files that fontdb finds under the family
//! names and styles the templates use, rendered by resvg at the axis
//! coordinates the SVG sets, and measured where they are rendered.

use std::sync::Arc;

use fontdb::{Database, Family, Query};
use runcard::render::Renderer;
use runcard::text::{Role, TextMeasure};

const REGULAR: &[u8] = include_bytes!("../fonts/InstrumentSerif-Regular.ttf");
const ITALIC: &[u8] = include_bytes!("../fonts/InstrumentSerif-Italic.ttf");
const BRICOLAGE: &[u8] = include_bytes!("../fonts/BricolageGrotesque-Variable.ttf");
const MONO: &[u8] = include_bytes!("../fonts/JetBrainsMono-Variable.ttf");

fn fonts(files: &[&[u8]]) -> Database {
    let mut db = Database::new();
    for data in files {
        db.load_font_data(data.to_vec());
    }
    db
}

fn bundled_fonts() -> Database {
    fonts(&[REGULAR, ITALIC, BRICOLAGE, MONO])
}

#[test]
fn the_bundled_fonts_provide_the_template_families_and_styles() {
    let db = bundled_fonts();
    assert_eq!(db.len(), 4);
    let style_of = |family: &str, style: fontdb::Style| {
        let id = db
            .query(&Query {
                families: &[Family::Name(family)],
                style,
                ..Query::default()
            })
            .unwrap_or_else(|| panic!("{family} {style:?} not found"));
        db.face(id).expect("a face").style
    };
    assert_eq!(
        style_of("Instrument Serif", fontdb::Style::Normal),
        fontdb::Style::Normal
    );
    assert_eq!(
        style_of("Instrument Serif", fontdb::Style::Italic),
        fontdb::Style::Italic
    );
    for family in ["Bricolage Grotesque", "JetBrains Mono"] {
        for weight in [400, 500, 600, 800] {
            let found = db.query(&Query {
                families: &[Family::Name(family)],
                weight: fontdb::Weight(weight),
                ..Query::default()
            });
            assert!(found.is_some(), "{family} at {weight} not found");
        }
    }
}

const PAPER: [u8; 3] = [0xff, 0xff, 0xff];

/// A white 720x200 card with `text` in black at x 20, baseline 120, `size` px.
fn probe_svg(attributes: &str, text: &str, size: f32) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="720" height="200" viewBox="0 0 720 200"><rect width="720" height="200" fill="white"/><text x="20" y="120" font-size="{size}" fill="black" {attributes}>{text}</text></svg>"#
    )
}

/// The RGB pixels of `svg` rendered at scale 1 with the fonts of `db`.
fn pixels_with(db: Database, svg: &str) -> Vec<u8> {
    let options = usvg::Options {
        fontdb: Arc::new(db),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_str(svg, &options).expect("the probe svg parses");
    let mut pixmap = tiny_skia::Pixmap::new(720, 200).expect("a pixmap");
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue()]
        })
        .collect()
}

/// The RGB pixels of `svg` rendered by the card renderer at scale 1.
fn pixels(svg: &str) -> Vec<u8> {
    let png = Renderer::new().unwrap().png(svg, 1.0).unwrap();
    let decoder = png::Decoder::new(std::io::Cursor::new(png));
    let mut reader = decoder.read_info().expect("a valid png header");
    let mut rgb = vec![0; reader.output_buffer_size().expect("a sane png size")];
    let info = reader.next_frame(&mut rgb).expect("a valid png frame");
    rgb.truncate(info.buffer_size());
    rgb
}

/// First and last column with a pixel that is not paper.
fn ink_extent(rgb: &[u8]) -> (u32, u32) {
    let (width, height) = (720, rgb.len() as u32 / 3 / 720);
    let inked = |x: u32| {
        (0..height).any(|y| {
            let i = ((y * width + x) * 3) as usize;
            rgb[i..i + 3] != PAPER
        })
    };
    let columns: Vec<u32> = (0..width).filter(|&x| inked(x)).collect();
    let (first, last) = (columns.first(), columns.last());
    (*first.expect("some ink"), *last.expect("some ink"))
}

fn ink_width(svg: &str) -> f32 {
    let (first, last) = ink_extent(&pixels(svg));
    (last - first + 1) as f32
}

/// How much ink: the sum of every channel's distance from white.
fn ink(svg: &str) -> u64 {
    pixels(svg).iter().map(|&c| u64::from(255 - c)).sum()
}

fn assert_tracks(measured: f32, inked: f32, what: &str) {
    eprintln!("{what}: measured {measured:.1} px, inked {inked} px");
    assert!(
        (measured - inked).abs() <= 0.05 * inked,
        "{what}: measured {measured} px, inked {inked} px"
    );
}

#[test]
fn font_style_italic_selects_the_italic_file() {
    let text = "Harbour Half";
    let family = r#"font-family="Instrument Serif""#;
    let italic = pixels(&probe_svg(&Role::DisplayItalic.attributes(), text, 60.0));
    let roman = pixels(&probe_svg(&Role::Display.attributes(), text, 60.0));
    // Each file alone is the only face of the family, whatever the style asks.
    let italic_file = pixels_with(fonts(&[ITALIC]), &probe_svg(family, text, 60.0));
    let regular_file = pixels_with(fonts(&[REGULAR]), &probe_svg(family, text, 60.0));
    assert!(
        italic == italic_file,
        "font-style italic is not the italic file"
    );
    assert!(
        roman == regular_file,
        "the normal style is not the regular file"
    );
    assert!(italic != roman, "the italic renders like the roman");
}

#[test]
fn the_bricolage_roles_render_at_their_coordinates_not_the_heavy_default() {
    let measure = TextMeasure::new();
    let heavy_default = r#"font-family="Bricolage Grotesque" font-variation-settings="'opsz' 96, 'wdth' 100, 'wght' 800""#;
    let text = "Harbour Half";
    let label = probe_svg(&Role::Label.attributes(), text, 40.0);
    let numeral = probe_svg(&Role::Numeral.attributes(), text, 40.0);
    let heavy = probe_svg(heavy_default, text, 40.0);
    // Ink per inked column: how heavy the strokes are, whatever the width.
    let density = |svg: &str| ink(svg) as f32 / ink_width(svg);
    let (label_weight, numeral_weight, heavy_weight) =
        (density(&label), density(&numeral), density(&heavy));
    eprintln!(
        "ink per column at 40 px: Label {label_weight:.0}, Numeral {numeral_weight:.0}, default {heavy_weight:.0}"
    );
    assert!(
        label_weight < 0.6 * numeral_weight,
        "a label inks {label_weight} per column, the numeral {numeral_weight}"
    );
    assert!(
        label_weight < 0.75 * heavy_weight,
        "a label inks {label_weight} per column, the default instance {heavy_weight}"
    );
    assert!(
        ink_width(&numeral) < 0.9 * ink_width(&heavy),
        "the numeral is not condensed"
    );
    for role in [Role::Numeral, Role::Label, Role::Text, Role::Strong] {
        let svg = probe_svg(&role.attributes(), text, 40.0);
        assert_tracks(
            measure.width(role, text, 40.0),
            ink_width(&svg),
            &format!("{text:?} in {role:?} at 40 px"),
        );
    }
}

#[test]
fn letter_spacing_tracks_the_text_as_width_spaced_measures_it() {
    let measure = TextMeasure::new();
    let cases = [
        (Role::Label, "SLOWER", 40.0, 8.0),
        (Role::Label, "LISBON \u{b7} 19 APRIL 2026", 22.0, 4.4),
        (Role::Numeral, "1:48:58", 132.0, -3.0),
    ];
    for (role, text, size, spacing) in cases {
        let attributes = format!(r#"{} letter-spacing="{spacing}""#, role.attributes());
        assert_tracks(
            measure.width_spaced(role, text, size, spacing),
            ink_width(&probe_svg(&attributes, text, size)),
            &format!("{text:?} in {role:?} at {size} px tracked by {spacing}"),
        );
    }
}

#[test]
fn the_measured_width_of_a_title_tracks_its_rendered_ink() {
    let measure = TextMeasure::new();
    let titles = [
        ("Harbour Half", 60.0),
        ("Maratona de Lisboa", 44.0),
        ("Schneider Electric Marathon de Paris", 28.0),
    ];
    for (text, size) in titles {
        let measured = measure.width(Role::Display, text, size);
        let inked = ink_width(&probe_svg(&Role::Display.attributes(), text, size));
        assert_tracks(
            measured,
            inked,
            &format!("{text:?} in Display at {size} px"),
        );
    }
    // The split title: the head in Display, the last word in a tspan in DisplayItalic.
    let split = format!(
        r#"Harbour <tspan {}>Half</tspan>"#,
        Role::DisplayItalic.attributes()
    );
    let measured = measure.width(Role::Display, "Harbour ", 60.0)
        + measure.width(Role::DisplayItalic, "Half", 60.0);
    let inked = ink_width(&probe_svg(&Role::Display.attributes(), &split, 60.0));
    assert_tracks(measured, inked, "Harbour *Half* at 60 px");
}
