//! Measures how wide a line of text is set in one of the type roles of the
//! card, so long names can be shrunk or cut before they run out of their room.

use skrifa::instance::{Location, Size};
use skrifa::{FontRef, MetadataProvider};

/// The four font files bundled into the binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// Instrument Serif, static: the title.
    InstrumentSerif,
    /// Instrument Serif Italic, static: the last word of the title and the byline.
    InstrumentSerifItalic,
    /// Bricolage Grotesque, axes `opsz` 12..96, `wdth` 75..100 and `wght`
    /// 200..800: the time, the labels and the stats.
    Bricolage,
    /// JetBrains Mono, axis `wght` 100..800: figures inside the pictures.
    JetBrainsMono,
}

/// Whether a face is upright or italic, as `font-style` selects it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Normal,
    Italic,
}

impl Face {
    /// Every face, in the order of [`TextMeasure`]'s fonts.
    pub const ALL: [Face; 4] = [
        Face::InstrumentSerif,
        Face::InstrumentSerifItalic,
        Face::Bricolage,
        Face::JetBrainsMono,
    ];

    /// The family name the templates and fontdb know the face by; both
    /// serif faces share theirs.
    pub fn family(self) -> &'static str {
        match self {
            Face::InstrumentSerif | Face::InstrumentSerifItalic => "Instrument Serif",
            Face::Bricolage => "Bricolage Grotesque",
            Face::JetBrainsMono => "JetBrains Mono",
        }
    }

    /// The style that selects this face within its family.
    pub fn style(self) -> Style {
        match self {
            Face::InstrumentSerifItalic => Style::Italic,
            _ => Style::Normal,
        }
    }

    /// The variation axes of the face, each of which its roles set.
    pub fn axes(self) -> &'static [&'static str] {
        match self {
            Face::InstrumentSerif | Face::InstrumentSerifItalic => &[],
            Face::Bricolage => &["opsz", "wdth", "wght"],
            Face::JetBrainsMono => &["wght"],
        }
    }

    /// The font file, included once for both measuring and rendering.
    pub(crate) fn data(self) -> &'static [u8] {
        match self {
            Face::InstrumentSerif => include_bytes!("../fonts/InstrumentSerif-Regular.ttf"),
            Face::InstrumentSerifItalic => include_bytes!("../fonts/InstrumentSerif-Italic.ttf"),
            Face::Bricolage => include_bytes!("../fonts/BricolageGrotesque-Variable.ttf"),
            Face::JetBrainsMono => include_bytes!("../fonts/JetBrainsMono-Variable.ttf"),
        }
    }
}

/// A type role of the card: a face at one fixed point of its design space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The title: Instrument Serif.
    Display,
    /// The last word of the title and the runner's byline: Instrument Serif Italic.
    DisplayItalic,
    /// The finish time: Bricolage at its display size, condensed, extra bold.
    Numeral,
    /// The subline and the legend captions: Bricolage at its smallest size, medium.
    Label,
    /// The stats line: Bricolage at text size, semibold.
    Text,
    /// The fastest pace in the stats line: Bricolage at text size, extra bold.
    Strong,
    /// Figures inside the pictures: JetBrains Mono, medium.
    Mono,
}

impl Role {
    /// Every role, in the order of the design system.
    pub const ALL: [Role; 7] = [
        Role::Display,
        Role::DisplayItalic,
        Role::Numeral,
        Role::Label,
        Role::Text,
        Role::Strong,
        Role::Mono,
    ];

    pub fn face(self) -> Face {
        match self {
            Role::Display => Face::InstrumentSerif,
            Role::DisplayItalic => Face::InstrumentSerifItalic,
            Role::Numeral | Role::Label | Role::Text | Role::Strong => Face::Bricolage,
            Role::Mono => Face::JetBrainsMono,
        }
    }

    /// The role's coordinates on every axis its face has, e.g.
    /// `[("opsz", 96), ("wdth", 75), ("wght", 800)]`; none for a static face.
    pub fn coordinates(self) -> &'static [(&'static str, f32)] {
        match self {
            Role::Display | Role::DisplayItalic => &[],
            Role::Numeral => &[("opsz", 96.0), ("wdth", 75.0), ("wght", 800.0)],
            Role::Label => &[("opsz", 12.0), ("wdth", 100.0), ("wght", 500.0)],
            Role::Text => &[("opsz", 14.0), ("wdth", 100.0), ("wght", 600.0)],
            Role::Strong => &[("opsz", 14.0), ("wdth", 100.0), ("wght", 800.0)],
            Role::Mono => &[("wght", 500.0)],
        }
    }

    /// The SVG attributes that set a text in this role, e.g.
    /// `font-family="Instrument Serif" font-style="italic"` or
    /// `font-family="JetBrains Mono" font-variation-settings="'wght' 500"`.
    /// The axes are set explicitly, so resvg renders exactly the measured
    /// instance; without `font-variation-settings` it would derive them from
    /// `font-weight`, the `font-stretch` keywords and the font size.
    pub fn attributes(self) -> String {
        let face = self.face();
        let mut attributes = format!(r#"font-family="{}""#, face.family());
        if face.style() == Style::Italic {
            attributes.push_str(r#" font-style="italic""#);
        }
        if !self.coordinates().is_empty() {
            let settings: Vec<String> = self
                .coordinates()
                .iter()
                .map(|(axis, value)| format!("'{axis}' {value}"))
                .collect();
            attributes.push_str(&format!(
                r#" font-variation-settings="{}""#,
                settings.join(", ")
            ));
        }
        attributes
    }
}

/// Horizontal text metrics of the bundled faces.
pub struct TextMeasure {
    fonts: [FontRef<'static>; 4],
}

impl TextMeasure {
    pub fn new() -> Self {
        let font = |face: Face| FontRef::new(face.data()).expect("the bundled fonts are valid");
        Self {
            fonts: Face::ALL.map(font),
        }
    }

    /// Width in pixels of `text` set in `role` at `size` px: the sum of the
    /// glyph advances at exactly the role's coordinates, a character without
    /// a glyph counting as the font's average advance.
    pub fn width(&self, role: Role, text: &str, size: f32) -> f32 {
        let font = &self.fonts[role.face() as usize];
        let location: Location = font.axes().location(role.coordinates().iter().copied());
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

    /// [`width`](Self::width) of a tracked text: `letter_spacing` px between
    /// every two characters. resvg also adds it after the last one, which
    /// leaves no ink, so a right-aligned tracked text ends that much early.
    pub fn width_spaced(&self, role: Role, text: &str, size: f32, letter_spacing: f32) -> f32 {
        let gaps = text.chars().count().saturating_sub(1);
        self.width(role, text, size) + letter_spacing * gaps as f32
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
    fn the_faces_are_the_four_bundled_files_the_serifs_sharing_a_family() {
        let faces: Vec<(Face, &str, Style, &[&str])> = Face::ALL
            .iter()
            .map(|&face| (face, face.family(), face.style(), face.axes()))
            .collect();
        assert_eq!(
            faces,
            [
                (
                    Face::InstrumentSerif,
                    "Instrument Serif",
                    Style::Normal,
                    &[][..]
                ),
                (
                    Face::InstrumentSerifItalic,
                    "Instrument Serif",
                    Style::Italic,
                    &[][..]
                ),
                (
                    Face::Bricolage,
                    "Bricolage Grotesque",
                    Style::Normal,
                    &["opsz", "wdth", "wght"][..]
                ),
                (
                    Face::JetBrainsMono,
                    "JetBrains Mono",
                    Style::Normal,
                    &["wght"][..]
                ),
            ]
        );
    }

    #[test]
    fn every_role_has_the_face_and_coordinates_of_the_design() {
        /// A role, its face and its coordinates.
        type Design = (Role, Face, &'static [(&'static str, f32)]);
        let roles: Vec<Design> = Role::ALL
            .iter()
            .map(|&role| (role, role.face(), role.coordinates()))
            .collect();
        assert_eq!(
            roles,
            [
                (Role::Display, Face::InstrumentSerif, &[][..]),
                (Role::DisplayItalic, Face::InstrumentSerifItalic, &[][..]),
                (
                    Role::Numeral,
                    Face::Bricolage,
                    &[("opsz", 96.0), ("wdth", 75.0), ("wght", 800.0)][..]
                ),
                (
                    Role::Label,
                    Face::Bricolage,
                    &[("opsz", 12.0), ("wdth", 100.0), ("wght", 500.0)][..]
                ),
                (
                    Role::Text,
                    Face::Bricolage,
                    &[("opsz", 14.0), ("wdth", 100.0), ("wght", 600.0)][..]
                ),
                (
                    Role::Strong,
                    Face::Bricolage,
                    &[("opsz", 14.0), ("wdth", 100.0), ("wght", 800.0)][..]
                ),
                (Role::Mono, Face::JetBrainsMono, &[("wght", 500.0)][..]),
            ]
        );
    }

    #[test]
    fn every_role_sets_each_axis_of_its_face() {
        for role in Role::ALL {
            let axes: Vec<&str> = role.coordinates().iter().map(|&(axis, _)| axis).collect();
            assert_eq!(axes, role.face().axes(), "{role:?}");
        }
    }

    #[test]
    fn a_role_sets_its_family_its_style_and_its_axes() {
        assert_eq!(
            Role::Display.attributes(),
            r#"font-family="Instrument Serif""#
        );
        assert_eq!(
            Role::DisplayItalic.attributes(),
            r#"font-family="Instrument Serif" font-style="italic""#
        );
        assert_eq!(
            Role::Numeral.attributes(),
            r#"font-family="Bricolage Grotesque" font-variation-settings="'opsz' 96, 'wdth' 75, 'wght' 800""#
        );
        assert_eq!(
            Role::Label.attributes(),
            r#"font-family="Bricolage Grotesque" font-variation-settings="'opsz' 12, 'wdth' 100, 'wght' 500""#
        );
        assert_eq!(
            Role::Text.attributes(),
            r#"font-family="Bricolage Grotesque" font-variation-settings="'opsz' 14, 'wdth' 100, 'wght' 600""#
        );
        assert_eq!(
            Role::Strong.attributes(),
            r#"font-family="Bricolage Grotesque" font-variation-settings="'opsz' 14, 'wdth' 100, 'wght' 800""#
        );
        assert_eq!(
            Role::Mono.attributes(),
            r#"font-family="JetBrains Mono" font-variation-settings="'wght' 500""#
        );
    }

    #[test]
    fn the_bundled_faces_have_the_axes_the_roles_use() {
        let axes = |face: Face| -> Vec<(String, f32, f32, f32)> {
            FontRef::new(face.data())
                .unwrap()
                .axes()
                .iter()
                .map(|axis| {
                    (
                        axis.tag().to_string(),
                        axis.min_value(),
                        axis.default_value(),
                        axis.max_value(),
                    )
                })
                .collect()
        };
        assert_eq!(axes(Face::InstrumentSerif), []);
        assert_eq!(axes(Face::InstrumentSerifItalic), []);
        // The default instance is the heavy display one, so every role sets
        // all three axes.
        assert_eq!(
            axes(Face::Bricolage),
            [
                ("opsz".to_owned(), 12.0, 96.0, 96.0),
                ("wght".to_owned(), 200.0, 800.0, 800.0),
                ("wdth".to_owned(), 75.0, 100.0, 100.0),
            ]
        );
        assert_eq!(
            axes(Face::JetBrainsMono),
            [("wght".to_owned(), 100.0, 400.0, 800.0)]
        );
    }

    #[test]
    fn mono_glyphs_advance_six_tenths_of_an_em() {
        // JetBrains Mono advances every glyph by 600 of its 1000 units per em.
        let width = TextMeasure::new().width(Role::Mono, "21", 11.0);
        assert_near(width, 2.0 * 0.6 * 11.0, 1e-3, "width of 21");
    }

    #[test]
    fn widths_scale_with_the_size_since_the_optical_size_belongs_to_the_role() {
        let measure = TextMeasure::new();
        for role in Role::ALL {
            let small = measure.width(role, "Harbour Half", 11.0);
            let large = measure.width(role, "Harbour Half", 44.0);
            assert!(small > 0.0, "{role:?} width was {small}");
            assert_near(
                large,
                4.0 * small,
                1e-3,
                &format!("{role:?} at four times the size"),
            );
        }
    }

    #[test]
    fn the_italic_the_weight_and_the_width_change_the_measure() {
        let measure = TextMeasure::new();
        let width = |role: Role| measure.width(role, "Harbour Half", 40.0);
        assert!(
            (width(Role::Display) - width(Role::DisplayItalic)).abs() > 0.02 * width(Role::Display),
            "the italic measures like the roman: {} and {}",
            width(Role::Display),
            width(Role::DisplayItalic)
        );
        assert!(
            width(Role::Text) < width(Role::Strong),
            "wght 600 is not narrower than 800"
        );
        assert!(
            width(Role::Numeral) < width(Role::Strong),
            "wdth 75 at opsz 96 is not narrower than wdth 100 at opsz 14"
        );
    }

    #[test]
    fn width_spaced_adds_the_letter_spacing_between_the_characters() {
        let measure = TextMeasure::new();
        let plain = measure.width(Role::Label, "SLOWER", 10.0);
        assert_near(
            measure.width_spaced(Role::Label, "SLOWER", 10.0, 1.5),
            plain + 5.0 * 1.5,
            1e-3,
            "SLOWER tracked by 1.5",
        );
        let time = measure.width(Role::Numeral, "1:48:58", 132.0);
        assert_near(
            measure.width_spaced(Role::Numeral, "1:48:58", 132.0, -3.0),
            time - 6.0 * 3.0,
            1e-3,
            "1:48:58 tracked by -3",
        );
        assert_near(
            measure.width_spaced(Role::Label, "\u{b7}", 10.0, 2.2),
            measure.width(Role::Label, "\u{b7}", 10.0),
            1e-3,
            "a single character",
        );
        assert_eq!(measure.width_spaced(Role::Label, "", 10.0, 2.2), 0.0);
    }

    #[test]
    fn a_character_without_a_glyph_counts_as_the_average_advance() {
        let measure = TextMeasure::new();
        // OS/2 average advances of 1000 units per em.
        assert_near(
            measure.width(Role::Text, "\u{1}", 100.0),
            53.5,
            1e-3,
            "Bricolage Grotesque",
        );
        assert_near(
            measure.width(Role::Display, "\u{1}", 100.0),
            42.2,
            1e-3,
            "Instrument Serif",
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
            (Role::Display, 76.0, "Harbour Half"),
            (Role::Display, 44.0, "Schneider Electric Marathon de Paris"),
            (Role::DisplayItalic, 76.0, "Harbour Half"),
            (Role::DisplayItalic, 26.0, "Maximilian Oberhauser-Lindqvist"),
            (Role::Numeral, 132.0, "1:48:58"),
            (Role::Label, 11.0, "LISBON \u{b7} 19 APRIL 2026"),
            // At 40 px resvg would set opsz 40 on its own; the role sets 12.
            (Role::Label, 40.0, "average pace"),
            (Role::Text, 17.0, "21.10 km"),
            (Role::Text, 40.0, "fastest km 21"),
            (Role::Strong, 17.0, "4:48"),
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
