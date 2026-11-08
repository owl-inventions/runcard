//! Measures how wide a line of text is set in one of the bundled fonts, so the
//! card can shrink or cut long names before they run off the card.

use skrifa::instance::Size;
use skrifa::{FontRef, MetadataProvider};

/// The three typefaces of the card template, bundled into the binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Font {
    /// IBM Plex Mono Medium: header, date, labels and the fastest-split line.
    Mono,
    /// Fraunces (variable): the event title.
    Serif,
    /// Bebas Neue: the runner's name and the stats.
    Condensed,
}

impl Font {
    /// Every bundled font, in the order of [`TextMeasure`]'s faces.
    pub(crate) const ALL: [Font; 3] = [Font::Mono, Font::Serif, Font::Condensed];

    /// The font file, included once for both measuring and rendering.
    pub(crate) fn data(self) -> &'static [u8] {
        match self {
            Font::Mono => include_bytes!("../fonts/IBMPlexMono-Medium.ttf"),
            Font::Serif => include_bytes!("../fonts/Fraunces-Variable.ttf"),
            Font::Condensed => include_bytes!("../fonts/BebasNeue-Regular.ttf"),
        }
    }

    /// The `font-weight` the card template sets this font in. It only matters
    /// for Fraunces, the one font with a weight axis.
    fn weight(self) -> f32 {
        match self {
            Font::Mono => 500.0,
            Font::Serif | Font::Condensed => 400.0,
        }
    }
}

/// Horizontal text metrics of the bundled fonts.
pub struct TextMeasure {
    faces: [FontRef<'static>; 3],
}

impl TextMeasure {
    pub fn new() -> Self {
        let face = |font: Font| FontRef::new(font.data()).expect("the bundled fonts are valid");
        Self {
            faces: Font::ALL.map(face),
        }
    }

    /// Width in pixels of `text` set in `font` at `size` px with
    /// `letter_spacing` px after every glyph: the sum of the glyph advances,
    /// a character without a glyph counting as the font's average advance.
    ///
    /// A variable font is measured where resvg renders it: at the template's
    /// weight and, like CSS `font-optical-sizing: auto`, at an optical size
    /// equal to `size`.
    pub fn width(&self, font: Font, text: &str, size: f32, letter_spacing: f32) -> f32 {
        let face = &self.faces[font as usize];
        let location = face
            .axes()
            .location([("wght", font.weight()), ("opsz", size)]);
        let metrics = face.metrics(Size::unscaled(), &location);
        let advances = face.glyph_metrics(Size::unscaled(), &location);
        let charmap = face.charmap();
        let units_per_em = f32::from(metrics.units_per_em);
        let average = metrics.average_width.unwrap_or(units_per_em / 2.0);
        let (units, glyphs) = text.chars().fold((0.0, 0.0), |(units, glyphs), c| {
            let advance = charmap
                .map(c)
                .and_then(|glyph| advances.advance_width(glyph))
                .unwrap_or(average);
            (units + advance, glyphs + 1.0)
        });
        units * size / units_per_em + letter_spacing * glyphs
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
        assert_eq!(truncate_to_fit("Berlin", at_most(6)), "Berlin");
    }

    #[test]
    fn a_text_that_does_not_fit_keeps_its_longest_fitting_start() {
        assert_eq!(
            truncate_to_fit("Berlin Marathon", at_most(6)),
            "Berli\u{2026}"
        );
        assert_eq!(
            truncate_to_fit("Berlin Marathon", at_most(14)),
            "Berlin Marath\u{2026}"
        );
    }

    #[test]
    fn spaces_before_the_ellipsis_are_dropped() {
        assert_eq!(
            truncate_to_fit("Berlin Marathon", at_most(8)),
            "Berlin\u{2026}"
        );
    }

    #[test]
    fn the_cut_falls_between_characters_not_bytes() {
        assert_eq!(truncate_to_fit("Zürich Äquator", at_most(3)), "Zü\u{2026}");
    }

    #[test]
    fn when_nothing_fits_only_the_ellipsis_is_left() {
        assert_eq!(truncate_to_fit("Berlin", at_most(0)), "\u{2026}");
    }

    #[test]
    fn mono_glyphs_advance_six_tenths_of_an_em_plus_the_letter_spacing() {
        // IBM Plex Mono advances every glyph by 600 of its 1000 units per em.
        let width = TextMeasure::new().width(Font::Mono, "RUN", 13.0, 3.0);
        assert_near(width, 3.0 * (0.6 * 13.0 + 3.0), 1e-3, "width of RUN");
    }

    #[test]
    fn widths_without_letter_spacing_scale_with_the_size() {
        let measure = TextMeasure::new();
        let small = measure.width(Font::Condensed, "JULIA BRANDT", 38.0, 0.0);
        let large = measure.width(Font::Condensed, "JULIA BRANDT", 76.0, 0.0);
        assert!(small > 0.0, "width was {small}");
        assert_near(large, 2.0 * small, 1e-3, "width at twice the size");
    }

    #[test]
    fn a_character_without_a_glyph_counts_as_the_average_advance() {
        let measure = TextMeasure::new();
        // OS/2 average advances: Bebas Neue 387, IBM Plex Mono 600 of 1000 units.
        assert_near(
            measure.width(Font::Condensed, "\u{1}", 100.0, 0.0),
            38.7,
            1e-3,
            "Bebas Neue",
        );
        assert_near(
            measure.width(Font::Mono, "\u{1}", 10.0, 0.0),
            6.0,
            1e-3,
            "IBM Plex Mono",
        );
    }

    /// The width resvg lays the text out at, from the bundled fonts, with the
    /// attributes the card template uses for that font.
    fn rendered_width(attributes: &str, text: &str) -> f32 {
        let mut fontdb = fontdb::Database::new();
        for font in Font::ALL {
            fontdb.load_font_source(fontdb::Source::Binary(std::sync::Arc::new(font.data())));
        }
        let options = usvg::Options {
            fontdb: std::sync::Arc::new(fontdb),
            ..usvg::Options::default()
        };
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="2000" height="200"><text x="0" y="100" {attributes}>{text}</text></svg>"#
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
            (
                Font::Mono,
                r#"font-family="IBM Plex Mono" font-weight="500" font-size="13" letter-spacing="3""#,
                13.0,
                3.0,
                "RUN · BERLIN",
            ),
            (
                Font::Serif,
                r#"font-family="Fraunces" font-size="48""#,
                48.0,
                0.0,
                "Schneider Electric Marathon de Paris",
            ),
            (
                Font::Serif,
                r#"font-family="Fraunces" font-size="28""#,
                28.0,
                0.0,
                "Schneider Electric Marathon de Paris",
            ),
            (
                Font::Condensed,
                r#"font-family="Bebas Neue" font-size="76""#,
                76.0,
                0.0,
                "MAXIMILIAN OBERHAUSER-LINDQVIST",
            ),
        ];
        for (font, attributes, size, spacing, text) in cases {
            // resvg adds no letter spacing after the last glyph.
            let measured = measure.width(font, text, size, spacing) - spacing;
            let rendered = rendered_width(attributes, text);
            assert_near(
                measured,
                rendered,
                0.01 * rendered,
                &format!("{font:?} {text:?} at {size}"),
            );
        }
    }
}
