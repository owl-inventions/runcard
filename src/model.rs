//! Shared data model passed between parsing, analysis, layout and rendering.

use serde::Serialize;
use time::{Date, OffsetDateTime};

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

/// The minijinja context for `templates/card.svg`: every value already laid out
/// (card pixel coordinates) and formatted (display strings).
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
