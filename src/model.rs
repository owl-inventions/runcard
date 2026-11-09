//! Shared data model passed between parsing, analysis, layout and rendering.

use serde::Serialize;
use time::{Date, OffsetDateTime};

use crate::layout::{bars::Bars, mosaic::Mosaic, rings::Rings, route_hero::RouteHero};

/// One recorded GPS fix.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackPoint {
    /// Latitude in decimal degrees.
    pub lat: f64,
    /// Longitude in decimal degrees.
    pub lon: f64,
    /// When the fix was taken, if the device recorded it.
    pub time: Option<OffsetDateTime>,
}

/// A parsed GPX track, all segments flattened into one point list.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    /// The track's `<name>`, if any.
    pub name: Option<String>,
    pub points: Vec<TrackPoint>,
}

/// One kilometre of the run (the last one may be shorter).
#[derive(Debug, Clone, PartialEq)]
pub struct Split {
    /// 1-based kilometre number.
    pub index: u32,
    /// Distance covered in this split, in metres (1000.0 unless partial).
    pub distance_m: f64,
    /// Time taken for this split, in seconds.
    pub seconds: f64,
    /// True for the trailing split that is shorter than a kilometre.
    pub partial: bool,
}

impl Split {
    /// Pace in seconds per kilometre, scaled up for a partial split.
    pub fn pace_s_per_km(&self) -> f64 {
        self.seconds / self.distance_m * 1000.0
    }
}

/// The analysed run: totals, splits and the points they were derived from.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    /// Total distance in metres.
    pub distance_m: f64,
    /// Total elapsed time in seconds.
    pub elapsed_s: f64,
    pub splits: Vec<Split>,
    /// `index` of the fastest full split, if there is one.
    pub fastest: Option<u32>,
    /// Timestamp of the first point.
    pub start: Option<OffsetDateTime>,
    pub points: Vec<TrackPoint>,
}

/// What the runner tells us about the run (CLI arguments).
#[derive(Debug, Clone, PartialEq)]
pub struct Meta {
    pub runner: String,
    pub event: String,
    pub city: Option<String>,
    pub date: Date,
}

/// One coloured band of the stripe bar.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stripe {
    /// Width in card pixels.
    pub width: f32,
    /// CSS colour, e.g. `#d94f30`.
    pub color: String,
}

/// One kilometre as the shape variants draw it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SplitView {
    /// 1-based kilometre number.
    pub index: u32,
    /// Pace without the unit, e.g. `5:02`.
    pub pace: String,
    /// CSS colour of the pace, the same as the kilometre's stripe.
    pub color: String,
    /// True for the trailing split that is shorter than a kilometre.
    pub partial: bool,
    /// Distance as a fraction of a kilometre, 1.0 for a full one.
    pub fraction: f32,
    /// Position of the pace on the colour scale, 0 slowest to 1 fastest.
    pub t: f32,
}

/// The colours of a card variant as CSS hex strings, e.g. `#f4efe6`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Theme {
    /// The background.
    pub paper: &'static str,
    /// Text, lines and markers.
    pub ink: &'static str,
    /// Secondary labels.
    pub muted: &'static str,
    /// Highlights such as the route start.
    pub accent: &'static str,
}

impl Theme {
    /// Near-black ink on cream paper with an orange accent.
    pub const fn light() -> Self {
        Self {
            paper: "#f4efe6",
            ink: "#1b1b1b",
            muted: "#8a8580",
            accent: "#e8601c",
        }
    }

    /// Cream ink on near-black paper with a red-orange accent.
    pub const fn dark() -> Self {
        Self {
            paper: "#0f1115",
            ink: "#f4efe6",
            muted: "#7d8590",
            accent: "#ff5a36",
        }
    }
}

/// The minijinja context for `templates/<variant>.svg`: every value already
/// laid out (card pixel coordinates) and formatted (display strings).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CardData {
    pub header: String,
    pub date: String,
    pub title: String,
    /// Font size of the title in px, smaller than 48 for a long event name.
    pub title_size: f32,
    pub runner: String,
    /// Font size of the runner's name in px, smaller than 76 for a long name.
    pub runner_size: f32,
    pub stripes: Vec<Stripe>,
    /// Horizontal position of the fastest-split marker.
    pub fastest_x: Option<f32>,
    /// Kilometre ticks under the stripe bar: (x position, kilometre number).
    pub ticks: Vec<(f32, u32)>,
    /// Route outline as a polyline in card coordinates.
    pub route: Vec<(f32, f32)>,
    /// Start marker of the route in card coordinates.
    pub route_start: (f32, f32),
    pub legend: Vec<String>,
    pub fastest_line: String,
    pub distance: String,
    pub time: String,
    pub pace: String,
    /// Name of the variant, which is also its template's name.
    pub variant: &'static str,
    pub theme: Theme,
    /// One view per split, in order.
    pub splits: Vec<SplitView>,
    /// Position of the average pace on the colour scale.
    pub avg_t: f32,
    /// Average pace over the whole distance without the unit, e.g. `5:39`.
    pub avg_pace: String,
    /// Shape blocks, each filled only for its own variant.
    pub rings: Option<Rings>,
    pub route_hero: Option<RouteHero>,
    pub bars: Option<Bars>,
    pub mosaic: Option<Mosaic>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(distance_m: f64, seconds: f64) -> Split {
        Split {
            index: 1,
            distance_m,
            seconds,
            partial: distance_m < 1000.0,
        }
    }

    #[test]
    fn pace_of_a_full_kilometre_is_its_time() {
        let pace = split(1000.0, 300.0).pace_s_per_km();
        assert!((pace - 300.0).abs() < 1e-9, "pace was {pace}");
    }

    #[test]
    fn pace_of_a_partial_split_is_scaled_to_a_kilometre() {
        let pace = split(195.0, 60.0).pace_s_per_km();
        assert!((pace - 307.7).abs() < 0.01, "pace was {pace}");
    }

    #[test]
    fn the_light_theme_is_ink_on_cream_with_an_orange_accent() {
        let theme = Theme::light();
        assert_eq!(
            (theme.paper, theme.ink, theme.muted, theme.accent),
            ("#f4efe6", "#1b1b1b", "#8a8580", "#e8601c")
        );
    }

    #[test]
    fn the_dark_theme_is_cream_on_near_black_with_a_red_accent() {
        let theme = Theme::dark();
        assert_eq!(
            (theme.paper, theme.ink, theme.muted, theme.accent),
            ("#0f1115", "#f4efe6", "#7d8590", "#ff5a36")
        );
    }
}
