//! The bundled fonts: real variable font files that fontdb finds under the
//! family names the templates use, rendered by resvg at the axis coordinates
//! the SVG sets, and measured where they are rendered.

use fontdb::{Database, Family, Query};
use runcard::render::Renderer;
use runcard::text::{Role, TextMeasure};

const FONTS: [&[u8]; 2] = [
    include_bytes!("../fonts/Archivo-Variable.ttf"),
    include_bytes!("../fonts/JetBrainsMono-Variable.ttf"),
];

fn bundled_fonts() -> Database {
    let mut db = Database::new();
    for data in FONTS {
        db.load_font_data(data.to_vec());
    }
    db
}

#[test]
fn the_bundled_fonts_provide_the_template_families_as_one_face_each() {
    let db = bundled_fonts();
    assert_eq!(db.len(), 2);
    for family in ["Archivo", "JetBrains Mono"] {
        for weight in [400, 500, 600, 700] {
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

/// First and last column with a pixel that is not paper, rendered at scale 1.
fn ink_extent(svg: &str) -> (u32, u32) {
    let png = Renderer::new().unwrap().png(svg, 1.0).unwrap();
    let decoder = png::Decoder::new(std::io::Cursor::new(png));
    let mut reader = decoder.read_info().expect("a valid png header");
    let mut rgb = vec![0; reader.output_buffer_size().expect("a sane png size")];
    let info = reader.next_frame(&mut rgb).expect("a valid png frame");
    let (width, height) = (info.width, info.height);
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
    let (first, last) = ink_extent(svg);
    (last - first + 1) as f32
}

#[test]
fn the_renderer_sets_archivo_at_the_width_the_svg_asks_for() {
    let at = |wdth: u32| {
        let attributes =
            format!(r#"font-family="Archivo" font-variation-settings="'wdth' {wdth}, 'wght' 600""#);
        ink_width(&probe_svg(&attributes, "Harbour", 40.0))
    };
    let (condensed, expanded) = (at(75), at(112));
    eprintln!(
        "Harbour at 40 px, wght 600: wdth 75 inks {condensed} px, wdth 112 inks {expanded} px"
    );
    assert!(
        expanded > 1.25 * condensed,
        "wdth 112 inks {expanded} px, wdth 75 {condensed} px"
    );
}

#[test]
fn the_measured_width_of_a_display_title_tracks_its_rendered_ink() {
    let measure = TextMeasure::new();
    let titles = [
        ("Harbour Half", 40.0),
        ("Maratona de Lisboa", 40.0),
        ("Schneider Electric Marathon de Paris", 28.0),
    ];
    for (text, size) in titles {
        let measured = measure.width(Role::Display, text, size);
        let inked = ink_width(&probe_svg(&Role::Display.attributes(), text, size));
        eprintln!("{text:?} in Display at {size} px: measured {measured:.1} px, inked {inked} px");
        assert!(
            (measured - inked).abs() <= 0.05 * inked,
            "{text:?}: measured {measured} px, inked {inked} px"
        );
    }
}
