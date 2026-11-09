//! Shared data model passed between parsing, analysis, layout and rendering.

use serde::Serialize;
use time::{Date, OffsetDateTime};

use crate::layout::{
    dots::Dots, ledger::Ledger, ribbon::Ribbon, spiral::Spiral, trail::Trail, wave::Wave,
};
use crate::theme::Tokens;

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

/// One kilometre as the shape variants draw it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SplitView {
    /// 1-based kilometre number.
    pub index: u32,
    /// Pace without the unit, e.g. `5:02`.
    pub pace: String,
    /// CSS colour of the pace on the palette.
    pub color: String,
    /// True for the trailing split that is shorter than a kilometre.
    pub partial: bool,
    /// Distance as a fraction of a kilometre, 1.0 for a full one.
    pub fraction: f32,
    /// Position of the pace on the colour scale, 0 slowest to 1 fastest.
    pub t: f32,
}

/// The route in miniature, drawn by the frame right of the legend.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoutePreview {
    /// SVG path data of the simplified route.
    pub path: String,
    /// Centre of the start marker, the first point of the route.
    pub start: (f32, f32),
}

/// The event title, its last word set apart in italic.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TitleView {
    /// Every word but the last, with the space before it, e.g. `Harbour `;
    /// the whole title when it has one word or is cut.
    pub head: String,
    /// The last word, e.g. `Half`; empty for a one-word or cut title.
    pub tail: String,
}

/// A figure and its unit, set in two colours.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Measure {
    /// The number, e.g. `21.10` or `5:43`.
    pub value: String,
    /// The unit after it, e.g. `km` or `/km`.
    pub unit: String,
}

/// The fastest full kilometre, flush right on the stats line.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Fastest {
    /// E.g. `fastest km 11`.
    pub label: String,
    /// Its pace without the unit, e.g. `5:02`.
    pub pace: String,
    /// Width of the pace in Strong at 17 px, which the label and the
    /// highlight are placed by.
    pub width: f32,
}

/// The stats line under the time: distance, average pace and the fastest km.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stats {
    /// The distance, e.g. `21.46` `km`.
    pub distance: Measure,
    /// The average pace over the whole distance, e.g. `5:43` `/km`.
    pub pace: Measure,
    /// Where the pace starts: 44 px after the distance.
    pub pace_x: f32,
    /// The fastest full kilometre; `None` without one.
    pub fastest: Option<Fastest>,
}

/// The minijinja context for `templates/<variant>.svg` and the shared
/// `templates/frame.svg`: every value already laid out (card pixels) and
/// formatted (display strings).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CardData {
    /// Name of the variant, which is also its template's name.
    pub variant: &'static str,
    /// The colours of the chosen theme.
    pub theme: Tokens,
    /// The event title, cut with an ellipsis and not split if it does not
    /// fit even at 44 px.
    pub title: TitleView,
    /// Font size of the title in px: 76, smaller for a long title, at least 44.
    pub title_size: f32,
    /// The runner's name as typed, cut with an ellipsis if it does not fit.
    pub runner: String,
    /// `<CITY> · <DATE>` under the byline in capitals, the city cut first to
    /// fit, e.g. `PARIS · 8 MARCH 2026`; just the date without a city.
    pub subline: String,
    /// The colours of the legend bar, slowest first.
    pub legend: Vec<String>,
    /// The route in miniature, flush right on the row of the legend; `None`
    /// for the `trail` variant, whose picture is the route, and for a track
    /// with fewer than two distinct points.
    pub route: Option<RoutePreview>,
    /// The finish time, e.g. `1:48:58`.
    pub time: String,
    /// Font size of the time in px: 132, smaller for a long time, at least 80.
    pub time_size: f32,
    /// The stats line under the time.
    pub stats: Stats,
    /// One view per split, in order.
    pub splits: Vec<SplitView>,
    /// Position of the average pace on the colour scale.
    pub avg_t: f32,
    /// Average pace over the whole distance without the unit, e.g. `5:43`.
    pub avg_pace: String,
    /// Shape blocks, each filled only for its own variant.
    pub ribbon: Option<Ribbon>,
    pub spiral: Option<Spiral>,
    pub trail: Option<Trail>,
    pub dots: Option<Dots>,
    pub wave: Option<Wave>,
    pub ledger: Option<Ledger>,
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
}
