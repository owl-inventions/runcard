//! The whole card pipeline, shared by the CLI and the server: GPX in,
//! analysis, layout, SVG and PNG out.

use std::io::Read;
use std::time::{Duration, Instant};

use time::macros::format_description;
use time::{Date, OffsetDateTime, UtcOffset};

use crate::error::Result;
use crate::model::Meta;
use crate::palette::Palette;
use crate::render::Renderer;
use crate::text::TextMeasure;
use crate::theme::Theme;
use crate::variant::Variant;
use crate::{card, gpx, splits};

/// Event name used when neither the request nor the GPX track names one.
const DEFAULT_EVENT: &str = "Run";

/// What the card shows besides the track, and how large it is rastered.
#[derive(Debug, Clone)]
pub struct CardRequest {
    /// Runner's name, shown as typed.
    pub runner: String,
    /// Event name; without one the track name, else "Run".
    pub event: Option<String>,
    /// City for the line under the title.
    pub city: Option<String>,
    /// Date of the run; without one the UTC date of the first timestamp, else today.
    pub date: Option<Date>,
    /// Raster scale over the 720x1000 card size.
    pub scale: f32,
    /// The card look.
    pub variant: Variant,
    /// The paper and ink, independent of the variant.
    pub theme: Theme,
}

/// The card as SVG and PNG, with the time spent in each stage.
#[derive(Debug, Clone)]
pub struct Rendered {
    pub svg: String,
    pub png: Vec<u8>,
    /// `parse`, `analyze`, `layout`, `svg` and `png`, in that order.
    pub timings: Vec<(&'static str, Duration)>,
}

/// Renderer, text metrics and palettes, built once and shared by every card.
pub struct Pipeline {
    renderer: Renderer,
    measure: TextMeasure,
    light: Palette,
    dark: Palette,
}

impl Pipeline {
    /// Loads the fonts and the card templates.
    pub fn new() -> Result<Self> {
        Ok(Self {
            renderer: Renderer::new()?,
            measure: TextMeasure::new(),
            light: Palette::for_theme(Theme::Light),
            dark: Palette::for_theme(Theme::Dark),
        })
    }

    /// The colour scale of `theme`.
    fn palette(&self, theme: Theme) -> &Palette {
        match theme {
            Theme::Light => &self.light,
            Theme::Dark => &self.dark,
        }
    }

    /// The card for the GPX track read from `gpx`, as SVG markup.
    pub fn svg(&self, gpx: impl Read, req: &CardRequest) -> Result<String> {
        self.timed_svg(gpx, req, &mut Stopwatch::start())
    }

    /// The card for the GPX track read from `gpx`, as PNG bytes at `req.scale`.
    pub fn png(&self, gpx: impl Read, req: &CardRequest) -> Result<Vec<u8>> {
        Ok(self.render(gpx, req)?.png)
    }

    /// The card as SVG and PNG, timing each stage.
    pub fn render(&self, gpx: impl Read, req: &CardRequest) -> Result<Rendered> {
        let mut clock = Stopwatch::start();
        let svg = self.timed_svg(gpx, req, &mut clock)?;
        let png = self.renderer.png(&svg, req.scale)?;
        clock.lap("png");
        Ok(Rendered {
            svg,
            png,
            timings: clock.stages,
        })
    }

    fn timed_svg(
        &self,
        gpx: impl Read,
        req: &CardRequest,
        clock: &mut Stopwatch,
    ) -> Result<String> {
        let track = gpx::parse(gpx)?;
        clock.lap("parse");

        let analysis = splits::analyze(&track)?;
        clock.lap("analyze");

        let meta = Meta {
            runner: req.runner.clone(),
            event: event_name(req.event.as_deref(), track.name.as_deref()),
            city: req.city.clone(),
            date: run_date(req.date, analysis.start, || {
                OffsetDateTime::now_utc().date()
            }),
        };
        let data = card::build(
            &analysis,
            &meta,
            self.palette(req.theme),
            &self.measure,
            req.variant,
            req.theme,
        );
        clock.lap("layout");

        let svg = self.renderer.svg(&data)?;
        clock.lap("svg");
        Ok(svg)
    }
}

/// Parses a date given as `YYYY-MM-DD`.
pub fn parse_date(s: &str) -> std::result::Result<Date, String> {
    Date::parse(s, format_description!("[year]-[month]-[day]"))
        .map_err(|e| format!("expected a date as YYYY-MM-DD: {e}"))
}

/// Parses a raster scale: a finite number above zero.
pub fn parse_scale(s: &str) -> std::result::Result<f32, String> {
    let scale: f32 = s.parse().map_err(|e| format!("not a number: {e}"))?;
    if scale.is_finite() && scale > 0.0 {
        Ok(scale)
    } else {
        Err(format!("must be a finite number above 0, got {scale}"))
    }
}

/// The requested event if given, else the track's name, else "Run".
fn event_name(requested: Option<&str>, track_name: Option<&str>) -> String {
    requested
        .or_else(|| track_name.map(str::trim).filter(|name| !name.is_empty()))
        .unwrap_or(DEFAULT_EVENT)
        .to_owned()
}

/// The requested date if given, else the UTC date of the first timestamp, else `today`.
fn run_date(
    requested: Option<Date>,
    start: Option<OffsetDateTime>,
    today: impl FnOnce() -> Date,
) -> Date {
    requested
        .or_else(|| start.map(|t| t.to_offset(UtcOffset::UTC).date()))
        .unwrap_or_else(today)
}

/// Wall-clock time per pipeline stage.
struct Stopwatch {
    last: Instant,
    stages: Vec<(&'static str, Duration)>,
}

impl Stopwatch {
    fn start() -> Self {
        Self {
            last: Instant::now(),
            stages: Vec::new(),
        }
    }

    /// Ends the stage `name` now and starts the next one.
    fn lap(&mut self, name: &'static str) {
        let now = Instant::now();
        self.stages.push((name, now - self.last));
        self.last = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::{date, datetime};

    #[test]
    fn a_date_is_parsed_from_year_month_day() {
        assert_eq!(parse_date("2026-09-27"), Ok(date!(2026 - 09 - 27)));
    }

    #[test]
    fn other_date_formats_are_rejected() {
        for input in ["27.09.2026", "2026-9-27", "2026-02-30", ""] {
            assert!(parse_date(input).is_err(), "{input:?} was accepted");
        }
    }

    #[test]
    fn the_scale_must_be_a_positive_number() {
        assert_eq!(parse_scale("2"), Ok(2.0));
        assert_eq!(parse_scale("0.5"), Ok(0.5));
        for input in ["0", "-1", "abc", "NaN", "inf"] {
            assert!(parse_scale(input).is_err(), "{input:?} was accepted");
        }
    }

    #[test]
    fn the_event_is_the_request_else_the_track_name_else_run() {
        assert_eq!(
            event_name(Some("Spreelauf"), Some("Morning Run")),
            "Spreelauf"
        );
        assert_eq!(event_name(None, Some("Berlin Marathon")), "Berlin Marathon");
        assert_eq!(event_name(None, Some("  ")), "Run");
        assert_eq!(event_name(None, None), "Run");
    }

    #[test]
    fn the_date_is_the_request_else_the_utc_start_date_else_today() {
        let today = || date!(2026 - 11 - 08);
        let given = Some(date!(2026 - 05 - 03));
        let start = Some(datetime!(2026-09-27 01:30 +02:00));
        assert_eq!(run_date(given, start, today), date!(2026 - 05 - 03));
        assert_eq!(run_date(None, start, today), date!(2026 - 09 - 26));
        assert_eq!(run_date(None, None, today), date!(2026 - 11 - 08));
    }
}
