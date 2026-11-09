//! The card variants: six encodings of the same splits inside the shared
//! frame, one template and one layout module each.

use std::fmt;
use std::str::FromStr;

/// A card look, selected by its lowercase name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Variant {
    #[default]
    Ribbon,
    Spiral,
    Trail,
    Dots,
    Wave,
    Ledger,
}

impl Variant {
    /// Every variant in display order, the default first.
    pub const ALL: [Variant; 6] = [
        Self::Ribbon,
        Self::Spiral,
        Self::Trail,
        Self::Dots,
        Self::Wave,
        Self::Ledger,
    ];

    /// The lowercase name: the CLI value, the form value and the template name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Ribbon => "ribbon",
            Self::Spiral => "spiral",
            Self::Trail => "trail",
            Self::Dots => "dots",
            Self::Wave => "wave",
            Self::Ledger => "ledger",
        }
    }

    /// One sentence on what the variant draws.
    pub fn description(self) -> &'static str {
        match self {
            Self::Ribbon => "One bar per kilometre, longer and brighter the faster it was run.",
            Self::Spiral => "The kilometres wound outward from the centre as one coloured spiral.",
            Self::Trail => "The route itself, each kilometre drawn in the colour of its pace.",
            Self::Dots => "A grid of dots, one per kilometre, larger for the faster ones.",
            Self::Wave => "The pace as a smooth curve over the distance, rising when faster.",
            Self::Ledger => "The splits as a table of figures, each pace set in its colour.",
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

    const NAMES: [&str; 6] = ["ribbon", "spiral", "trail", "dots", "wave", "ledger"];

    #[test]
    fn all_lists_the_variants_in_display_order_with_their_names() {
        let names: Vec<&str> = Variant::ALL.iter().map(|v| v.name()).collect();
        assert_eq!(names, NAMES);
        assert_eq!(
            Variant::ALL,
            [
                Variant::Ribbon,
                Variant::Spiral,
                Variant::Trail,
                Variant::Dots,
                Variant::Wave,
                Variant::Ledger,
            ]
        );
    }

    #[test]
    fn every_name_parses_and_displays_as_itself() {
        for (variant, name) in Variant::ALL.into_iter().zip(NAMES) {
            assert_eq!(name.parse::<Variant>(), Ok(variant), "{name}");
            assert_eq!(variant.to_string(), name);
        }
    }

    #[test]
    fn an_unknown_or_capitalised_name_is_an_error_listing_the_names() {
        assert_eq!(
            "x".parse::<Variant>(),
            Err(r#"expected one of ribbon, spiral, trail, dots, wave, ledger, got "x""#.to_owned())
        );
        for name in ["Ribbon", "WAVE", " dots", "", "classic"] {
            assert!(name.parse::<Variant>().is_err(), "{name:?} was accepted");
        }
    }

    #[test]
    fn ribbon_is_the_default() {
        assert_eq!(Variant::default(), Variant::Ribbon);
    }

    #[test]
    fn every_variant_has_a_short_one_sentence_description() {
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
            assert!(description.len() <= 80, "{variant}: {description:?}");
        }
    }
}
