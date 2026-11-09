//! The two colour themes of the card, chosen independently of the variant.

use std::fmt;
use std::str::FromStr;

use serde::Serialize;

/// The paper and ink of the card, selected by its lowercase name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    /// Near-black ink on pale paper.
    #[default]
    Light,
    /// Cream ink on near-black paper, the visualisation glowing.
    Dark,
}

/// The colours of a theme as CSS hex strings, e.g. `#f6f6f4`, and whether
/// it glows: what the templates read as `theme.paper`, `theme.ink`,
/// `theme.muted`, `theme.rule`, `theme.accent`, `theme.marker`,
/// `theme.on_fast` and `theme.dark`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Tokens {
    /// The background.
    pub paper: &'static str,
    /// Text and markers.
    pub ink: &'static str,
    /// Labels and secondary text.
    pub muted: &'static str,
    /// Hairlines.
    pub rule: &'static str,
    /// The fastest pace in the footer and its highlight.
    pub accent: &'static str,
    /// The fastest-km marker and its label inside the picture: the accent on
    /// light paper, the ink on dark, whose palette already ends in coral.
    pub marker: &'static str,
    /// Text set on the fastest kilometre's own colour.
    pub on_fast: &'static str,
    /// Dark paper: the frame draws a blurred copy of the visualisation under
    /// it, so the shapes glow.
    pub dark: bool,
}

impl Theme {
    /// Both themes, the default first.
    pub const ALL: [Theme; 2] = [Self::Light, Self::Dark];

    /// The lowercase name: the CLI value and the form value.
    pub fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// The colours of the theme.
    pub fn tokens(self) -> Tokens {
        match self {
            Self::Light => Tokens {
                paper: "#f6f6f4",
                ink: "#17191d",
                muted: "#8a8d93",
                rule: "#d9dad6",
                accent: "#e2492a",
                marker: "#e2492a",
                on_fast: "#17191d",
                dark: false,
            },
            Self::Dark => Tokens {
                paper: "#0f1115",
                ink: "#f4efe6",
                muted: "#7d8590",
                rule: "#262b33",
                accent: "#ff5a36",
                marker: "#f4efe6",
                on_fast: "#0f1115",
                dark: true,
            },
        }
    }
}

impl fmt::Display for Theme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Theme {
    type Err = String;

    /// The theme with the lowercase name `s`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|theme| theme.name() == s)
            .ok_or_else(|| format!("expected light or dark, got {s:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_and_dark_parse_and_display_as_themselves() {
        for (theme, name) in [(Theme::Light, "light"), (Theme::Dark, "dark")] {
            assert_eq!(name.parse::<Theme>(), Ok(theme));
            assert_eq!(theme.to_string(), name);
        }
    }

    #[test]
    fn any_other_name_is_an_error_naming_both_themes() {
        assert_eq!(
            "x".parse::<Theme>(),
            Err(r#"expected light or dark, got "x""#.to_owned())
        );
        for name in ["Dark", "LIGHT", " dark", ""] {
            assert!(name.parse::<Theme>().is_err(), "{name:?} was accepted");
        }
    }

    #[test]
    fn light_is_the_default() {
        assert_eq!(Theme::default(), Theme::Light);
    }

    #[test]
    fn all_lists_light_then_dark() {
        assert_eq!(Theme::ALL, [Theme::Light, Theme::Dark]);
    }

    #[test]
    fn the_light_tokens_are_ink_on_pale_paper_with_a_vermilion_accent_without_a_glow() {
        assert_eq!(
            Theme::Light.tokens(),
            Tokens {
                paper: "#f6f6f4",
                ink: "#17191d",
                muted: "#8a8d93",
                rule: "#d9dad6",
                accent: "#e2492a",
                marker: "#e2492a",
                on_fast: "#17191d",
                dark: false,
            }
        );
    }

    #[test]
    fn the_dark_tokens_are_cream_on_near_black_with_a_coral_accent_and_a_glow() {
        assert_eq!(
            Theme::Dark.tokens(),
            Tokens {
                paper: "#0f1115",
                ink: "#f4efe6",
                muted: "#7d8590",
                rule: "#262b33",
                accent: "#ff5a36",
                marker: "#f4efe6",
                on_fast: "#0f1115",
                dark: true,
            }
        );
    }

    #[test]
    fn the_dark_marker_is_the_cream_ink_since_the_fast_end_of_its_palette_is_coral() {
        let dark = Theme::Dark.tokens();
        assert_eq!(dark.marker, dark.ink);
        assert_ne!(dark.marker, dark.accent);
        let light = Theme::Light.tokens();
        assert_eq!(light.marker, light.accent);
    }
}
