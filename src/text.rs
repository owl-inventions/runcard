//! Measures how wide a line of text is set in one of the type roles of the
//! card, so long names can be shrunk or cut before they run out of their room.

use skrifa::instance::{Location, Size};
use skrifa::{FontRef, MetadataProvider};

/// The two typefaces of the card, both variable fonts bundled into the binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// Archivo, axes `wdth` 62..125 and `wght` 100..900: every text of the frame.
    Archivo,
    /// JetBrains Mono, axis `wght` 100..800: figures inside the visualisations.
    JetBrainsMono,
}

impl Face {
    /// Both faces, in the order of [`TextMeasure`]'s fonts.
    pub const ALL: [Face; 2] = [Face::Archivo, Face::JetBrainsMono];

    /// The family name the templates and fontdb know the face by.
    pub fn family(self) -> &'static str {
        match self {
            Face::Archivo => "Archivo",
            Face::JetBrainsMono => "JetBrains Mono",
        }
    }

    /// The variation axes the roles set on this face.
    pub fn axes(self) -> &'static [&'static str] {
        match self {
            Face::Archivo => &["wdth", "wght"],
            Face::JetBrainsMono => &["wght"],
        }
    }

    /// The font file, included once for both measuring and rendering.
    pub(crate) fn data(self) -> &'static [u8] {
        match self {
            Face::Archivo => include_bytes!("../fonts/Archivo-Variable.ttf"),
            Face::JetBrainsMono => include_bytes!("../fonts/JetBrainsMono-Variable.ttf"),
        }
    }
}

/// A type role of the card: a face at one fixed point of its design space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The event title: Archivo semi-expanded, semibold.
    Display,
    /// The big finish time: Archivo condensed, medium.
    Numeral,
    /// Small labels and the runner's name: Archivo, medium.
    Label,
    /// The values in the footer: Archivo, regular.
    Text,
    /// Kilometre numbers and paces inside the visualisations: JetBrains Mono, medium.
    Mono,
}

impl Role {
    /// Every role, in the order of the design system.
    pub const ALL: [Role; 5] = [
        Role::Display,
        Role::Numeral,
        Role::Label,
        Role::Text,
        Role::Mono,
    ];

    pub fn face(self) -> Face {
        match self {
            Role::Mono => Face::JetBrainsMono,
            _ => Face::Archivo,
        }
    }

    /// The `wdth` coordinate, 100 for the normal width.
    pub fn wdth(self) -> f32 {
        match self {
            Role::Display => 112.0,
            Role::Numeral => 85.0,
            Role::Label | Role::Text | Role::Mono => 100.0,
        }
    }

    /// The `wght` coordinate.
    pub fn wght(self) -> f32 {
        match self {
            Role::Display => 600.0,
            Role::Text => 400.0,
            Role::Numeral | Role::Label | Role::Mono => 500.0,
        }
    }

    /// The role's coordinates on the axes its face has, e.g. `[("wdth", 112),
    /// ("wght", 600)]`.
    pub fn coordinates(self) -> Vec<(&'static str, f32)> {
        self.face()
            .axes()
            .iter()
            .map(|&axis| {
                (
                    axis,
                    if axis == "wdth" {
                        self.wdth()
                    } else {
                        self.wght()
                    },
                )
            })
            .collect()
    }

    /// The SVG attributes that set a text in this role, e.g.
    /// `font-family="Archivo" font-variation-settings="'wdth' 112, 'wght' 600"`.
    /// The axes are set explicitly, so resvg renders exactly the measured
    /// instance; without `font-variation-settings` it would derive them from
    /// `font-weight` and the `font-stretch` keywords.
    pub fn attributes(self) -> String {
        let settings: Vec<String> = self
            .coordinates()
            .iter()
            .map(|(axis, value)| format!("'{axis}' {value}"))
            .collect();
        format!(
            r#"font-family="{}" font-variation-settings="{}""#,
            self.face().family(),
            settings.join(", ")
        )
    }
}

/// Horizontal text metrics of the bundled faces.
pub struct TextMeasure {
    fonts: [FontRef<'static>; 2],
}

impl TextMeasure {
    pub fn new() -> Self {
        let font = |face: Face| FontRef::new(face.data()).expect("the bundled fonts are valid");
        Self {
            fonts: Face::ALL.map(font),
        }
    }

    /// Width in pixels of `text` set in `role` at `size` px: the sum of the
    /// glyph advances at the role's coordinates, a character without a glyph
    /// counting as the font's average advance.
    ///
    /// Like resvg (CSS `font-optical-sizing: auto`), an `opsz` axis would be
    /// set to `size`; skrifa ignores it for the bundled faces, which have none.
    pub fn width(&self, role: Role, text: &str, size: f32) -> f32 {
        let font = &self.fonts[role.face() as usize];
        let location: Location = font
            .axes()
            .location(role.coordinates().into_iter().chain([("opsz", size)]));
        let metrics = font.metrics(Size::unscaled(), &location);
        let advances = font.glyph_metrics(Size::unscaled(), &location);
        let charmap = font.charmap();
        let units_per_em = f32::from(metrics.units_per_em);
        let average = metrics.average_width.unwrap_or(units_per_em / 2.0);
        let units: f32 = text
            .chars()
            .map(|c| {
                charmap
                    .map(c)
                    .and_then(|glyph| advances.advance_width(glyph))
                    .unwrap_or(average)
            })
            .sum();
        units * size / units_per_em
    }
}

impl Default for TextMeasure {
    fn default() -> Self {
        Self::new()
    }
}

/// The mark that ends a cut text.
pub const ELLIPSIS: char = '\u{2026}';

/// `text` if `fits` accepts it, else its longest start that `fits` accepts
/// once cut with a single [`ELLIPSIS`]; spaces before the ellipsis are dropped.
/// Just the ellipsis when not even one character fits.
///
/// `fits` must not accept a text after rejecting a shorter start of it.
pub fn truncate_to_fit(text: &str, fits: impl Fn(&str) -> bool) -> String {
    if fits(text) {
        return text.to_owned();
    }
    let cut = |chars: usize| {
        let end = text
            .char_indices()
            .nth(chars)
            .map_or(text.len(), |(i, _)| i);
        format!("{}{ELLIPSIS}", text[..end].trim_end())
    };
    // Bisect the number of characters kept: `cut(fitting)` fits or keeps
    // nothing; keeping all `too_many` characters would be no cut at all.
    let (mut fitting, mut too_many) = (0, text.chars().count());
    while too_many - fitting > 1 {
        let mid = (fitting + too_many) / 2;
        if fits(&cut(mid)) {
            fitting = mid;
        } else {
            too_many = mid;
        }
    }
    cut(fitting)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near(actual: f32, expected: f32, tolerance: f32, what: &str) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "{what} was {actual}, expected {expected} ± {tolerance}"
        );
    }

    /// Accepts texts of at most `max` characters.
    fn at_most(max: usize) -> impl Fn(&str) -> bool {
        move |text: &str| text.chars().count() <= max
    }

    #[test]
    fn a_text_that_fits_is_not_cut() {
        assert_eq!(truncate_to_fit("Lisbon", at_most(6)), "Lisbon");
    }

    #[test]
    fn a_text_that_does_not_fit_keeps_its_longest_fitting_start() {
        assert_eq!(truncate_to_fit("Harbour Half", at_most(6)), "Harbo\u{2026}");
        assert_eq!(
            truncate_to_fit("Harbour Half", at_most(11)),
            "Harbour Ha\u{2026}"
        );
    }

    #[test]
    fn spaces_before_the_ellipsis_are_dropped() {
        assert_eq!(
            truncate_to_fit("Harbour Half", at_most(9)),
            "Harbour\u{2026}"
        );
    }

    #[test]
    fn the_cut_falls_between_characters_not_bytes() {
        assert_eq!(truncate_to_fit("Zürich Äquator", at_most(3)), "Zü\u{2026}");
    }

    #[test]
    fn when_nothing_fits_only_the_ellipsis_is_left() {
        assert_eq!(truncate_to_fit("Lisbon", at_most(0)), "\u{2026}");
    }

    #[test]
    fn every_role_has_the_face_width_and_weight_of_the_design() {
        let roles: Vec<(Role, Face, f32, f32)> = Role::ALL
            .iter()
            .map(|&role| (role, role.face(), role.wdth(), role.wght()))
            .collect();
        assert_eq!(
            roles,
            [
                (Role::Display, Face::Archivo, 112.0, 600.0),
                (Role::Numeral, Face::Archivo, 85.0, 500.0),
                (Role::Label, Face::Archivo, 100.0, 500.0),
                (Role::Text, Face::Archivo, 100.0, 400.0),
                (Role::Mono, Face::JetBrainsMono, 100.0, 500.0),
            ]
        );
    }

    #[test]
    fn a_role_sets_its_family_and_every_axis_its_face_has() {
        assert_eq!(
            Role::Display.attributes(),
            r#"font-family="Archivo" font-variation-settings="'wdth' 112, 'wght' 600""#
        );
        assert_eq!(
            Role::Numeral.attributes(),
            r#"font-family="Archivo" font-variation-settings="'wdth' 85, 'wght' 500""#
        );
        assert_eq!(
            Role::Mono.attributes(),
            r#"font-family="JetBrains Mono" font-variation-settings="'wght' 500""#
        );
    }

    #[test]
    fn the_bundled_faces_are_variable_with_the_axes_the_roles_use() {
        let axes = |face: Face| -> Vec<(String, f32, f32)> {
            FontRef::new(face.data())
                .unwrap()
                .axes()
                .iter()
                .map(|axis| (axis.tag().to_string(), axis.min_value(), axis.max_value()))
                .collect()
        };
        assert_eq!(
            axes(Face::Archivo),
            [
                ("wght".to_owned(), 100.0, 900.0),
                ("wdth".to_owned(), 62.0, 125.0)
            ]
        );
        assert_eq!(
            axes(Face::JetBrainsMono),
            [("wght".to_owned(), 100.0, 800.0)]
        );
        assert_eq!(Face::Archivo.axes(), ["wdth", "wght"]);
        assert_eq!(Face::JetBrainsMono.axes(), ["wght"]);
    }

    #[test]
    fn mono_glyphs_advance_six_tenths_of_an_em() {
        // JetBrains Mono advances every glyph by 600 of its 1000 units per em.
        let width = TextMeasure::new().width(Role::Mono, "21", 11.0);
        assert_near(width, 2.0 * 0.6 * 11.0, 1e-3, "width of 21");
    }

    #[test]
    fn widths_scale_with_the_size() {
        let measure = TextMeasure::new();
        for role in Role::ALL {
            let small = measure.width(role, "Harbour Half", 20.0);
            let large = measure.width(role, "Harbour Half", 40.0);
            assert!(small > 0.0, "{role:?} width was {small}");
            assert_near(
                large,
                2.0 * small,
                1e-3,
                &format!("{role:?} at twice the size"),
            );
        }
    }

    #[test]
    fn the_width_axis_and_the_weight_widen_the_text() {
        let measure = TextMeasure::new();
        let width = |role: Role| measure.width(role, "Harbour Half", 40.0);
        assert!(
            width(Role::Numeral) < width(Role::Label),
            "wdth 85 is not narrower than 100"
        );
        assert!(
            width(Role::Text) < width(Role::Label),
            "wght 400 is not narrower than 500"
        );
        assert!(
            width(Role::Label) < width(Role::Display),
            "wdth 112 is not wider than 100"
        );
    }

    #[test]
    fn a_character_without_a_glyph_counts_as_the_average_advance() {
        let measure = TextMeasure::new();
        // OS/2 average advances: Archivo 575, JetBrains Mono 602 of 1000 units.
        assert_near(
            measure.width(Role::Text, "\u{1}", 100.0),
            57.5,
            1e-3,
            "Archivo",
        );
        assert_near(
            measure.width(Role::Mono, "\u{1}", 10.0),
            6.02,
            1e-3,
            "JetBrains Mono",
        );
    }

    /// The width resvg lays `text` out at in `role` at `size`, from the
    /// bundled faces.
    fn rendered_width(role: Role, size: f32, text: &str) -> f32 {
        let mut fontdb = fontdb::Database::new();
        for face in Face::ALL {
            fontdb.load_font_source(fontdb::Source::Binary(std::sync::Arc::new(face.data())));
        }
        let options = usvg::Options {
            fontdb: std::sync::Arc::new(fontdb),
            ..usvg::Options::default()
        };
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="2000" height="200"><text x="0" y="100" {} font-size="{size}">{text}</text></svg>"#,
            role.attributes()
        );
        let tree = usvg::Tree::from_str(&svg, &options).expect("the probe svg parses");
        let Some(usvg::Node::Text(text)) = tree.root().children().first() else {
            panic!("no text node in {svg}");
        };
        text.bounding_box().width()
    }

    #[test]
    fn measured_widths_match_the_rendered_layout() {
        let measure = TextMeasure::new();
        let cases = [
            (Role::Display, 40.0, "Harbour Half"),
            (Role::Display, 28.0, "Schneider Electric Marathon de Paris"),
            (Role::Numeral, 72.0, "1:48:58"),
            (Role::Label, 12.0, "Lisbon \u{b7} 19 April 2026"),
            (Role::Label, 14.0, "Maximilian Oberhauser-Lindqvist"),
            (Role::Text, 20.0, "21.10 km"),
            (Role::Mono, 11.0, "21"),
        ];
        for (role, size, text) in cases {
            let measured = measure.width(role, text, size);
            let rendered = rendered_width(role, size, text);
            assert_near(
                measured,
                rendered,
                0.01 * rendered,
                &format!("{role:?} {text:?} at {size}"),
            );
        }
    }
}
