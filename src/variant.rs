//! The card variants: six looks for the same card data, one template each.

use std::fmt;
use std::str::FromStr;

use crate::model::Theme;

/// A card look, selected by its lowercase name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Variant {
    #[default]
    Classic,
    Noir,
    Rings,
    Route,
    Bars,
    Mosaic,
}

impl Variant {
    /// Every variant in display order, the default first.
    pub const ALL: [Variant; 6] = [
        Self::Classic,
        Self::Noir,
        Self::Rings,
        Self::Route,
        Self::Bars,
        Self::Mosaic,
    ];

    /// The lowercase name: the CLI value, the form value and the template name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Noir => "noir",
            Self::Rings => "rings",
            Self::Route => "route",
            Self::Bars => "bars",
            Self::Mosaic => "mosaic",
        }
    }

    /// One sentence on how the variant looks.
    pub fn description(self) -> &'static str {
        match self {
            Self::Classic => "One coloured stripe per kilometre on cream paper, slate blue for slow to orange for fast.",
            Self::Noir => "The classic stripes on a near-black card with a soft glow.",
            Self::Rings => "The kilometres as a ring of coloured segments around the finish time.",
            Self::Route => "The route drawn large, each kilometre in its pace colour.",
            Self::Bars => "A speed chart with one bar per kilometre and a dashed average line.",
            Self::Mosaic => "One tile per kilometre in a calendar-like grid, each with its pace.",
        }
    }

    /// Whether the variant is set on the dark theme with the dark palette.
    pub fn is_dark(self) -> bool {
        self == Self::Noir
    }

    /// The paper, ink, muted and accent colours of the variant.
    pub fn theme(self) -> Theme {
        if self.is_dark() {
            Theme::dark()
        } else {
            Theme::light()
        }
    }
}

impl fmt::Display for Variant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Variant {
    type Err = String;

    /// The variant with the lowercase name `s`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|variant| variant.name() == s)
            .ok_or_else(|| {
                let names: Vec<&str> = Self::ALL.iter().map(|v| v.name()).collect();
                format!("expected one of {}, got {s:?}", names.join(", "))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Theme;

    const NAMES: [&str; 6] = ["classic", "noir", "rings", "route", "bars", "mosaic"];

    #[test]
    fn all_lists_the_variants_in_display_order_with_their_names() {
        let names: Vec<&str> = Variant::ALL.iter().map(|v| v.name()).collect();
        assert_eq!(names, NAMES);
    }

    #[test]
    fn every_name_parses_and_displays_as_itself() {
        for (variant, name) in Variant::ALL.into_iter().zip(NAMES) {
            assert_eq!(name.parse::<Variant>(), Ok(variant), "{name}");
            assert_eq!(variant.to_string(), name);
            assert_eq!(variant.to_string().parse::<Variant>(), Ok(variant));
        }
    }

    #[test]
    fn an_unknown_or_capitalised_name_is_an_error_listing_the_names() {
        assert_eq!(
            "x".parse::<Variant>(),
            Err(r#"expected one of classic, noir, rings, route, bars, mosaic, got "x""#.to_owned())
        );
        for name in ["Noir", "NOIR", " noir", ""] {
            assert!(name.parse::<Variant>().is_err(), "{name:?} was accepted");
        }
    }

    #[test]
    fn classic_is_the_default() {
        assert_eq!(Variant::default(), Variant::Classic);
    }

    #[test]
    fn every_variant_has_a_one_sentence_description() {
        for variant in Variant::ALL {
            let description = variant.description();
            assert!(
                description.ends_with('.') && description.matches(". ").count() == 0,
                "{variant}: {description:?}"
            );
            assert!(
                description.starts_with(char::is_uppercase),
                "{variant}: {description:?}"
            );
        }
    }

    #[test]
    fn noir_is_dark_and_every_other_variant_light() {
        for variant in Variant::ALL {
            let dark = variant == Variant::Noir;
            assert_eq!(variant.is_dark(), dark, "{variant}");
            let expected = if dark { Theme::dark() } else { Theme::light() };
            assert_eq!(variant.theme(), expected, "{variant}");
        }
    }
}
