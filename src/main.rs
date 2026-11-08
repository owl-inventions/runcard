//! `runcard <INPUT.gpx> --runner <NAME>`: renders a GPX track as a PNG card.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use runcard::palette::Palette;
use runcard::render::Renderer;
use runcard::text::TextMeasure;
use runcard::{card, gpx, splits, Meta};
use time::macros::format_description;
use time::{Date, OffsetDateTime, UtcOffset};

/// Event name used when neither `--event` nor the GPX track names one.
const DEFAULT_EVENT: &str = "Run";

/// Turns a GPX track into a poster-style PNG card: one coloured stripe per
/// kilometre, the runner's name, the route outline and the race stats.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// The GPX file of the run.
    #[arg(value_name = "INPUT.gpx")]
    input: PathBuf,
    /// Runner's name, printed in capitals.
    #[arg(long, value_name = "NAME")]
    runner: String,
    /// Event name [default: the GPX track name, else "Run"].
    #[arg(long, value_name = "NAME")]
    event: Option<String>,
    /// City for the header line.
    #[arg(long, value_name = "CITY")]
    city: Option<String>,
    /// Date of the run [default: the UTC date of the first timestamp, else today].
    #[arg(long, value_name = "YYYY-MM-DD", value_parser = parse_date)]
    date: Option<Date>,
    /// PNG to write [default: the input path with a .png extension].
    #[arg(short, long, value_name = "OUT.png")]
    output: Option<PathBuf>,
    /// Raster scale over the 720x1000 card size.
    #[arg(long, value_name = "F", default_value_t = 2.0, value_parser = parse_scale)]
    scale: f32,
    /// Also write the SVG to this path.
    #[arg(long, value_name = "OUT.svg")]
    svg: Option<PathBuf>,
    /// Print the time spent in each stage to stderr.
    #[arg(long)]
    timing: bool,
}

fn main() -> Result<()> {
    run(Args::parse())
}

/// The pipeline: GPX -> analysis -> card layout -> SVG -> PNG on disk.
fn run(args: Args) -> Result<()> {
    let mut clock = Stopwatch::start();
    let input = args.input;
    let track = gpx::load(&input).with_context(|| format!("reading {}", input.display()))?;
    clock.lap("parse");

    let analysis =
        splits::analyze(&track).with_context(|| format!("analysing {}", input.display()))?;
    clock.lap("analyze");

    let meta = Meta {
        runner: args.runner,
        event: event_name(args.event, track.name.as_deref()),
        city: args.city,
        date: run_date(args.date, analysis.start, || {
            OffsetDateTime::now_utc().date()
        }),
    };
    let data = card::build(&analysis, &meta, &Palette::new(), &TextMeasure::new());
    clock.lap("layout");

    let renderer = Renderer::new().context("loading the card template and fonts")?;
    let svg = renderer.svg(&data).context("filling the card template")?;
    if let Some(path) = &args.svg {
        fs::write(path, &svg).with_context(|| format!("writing {}", path.display()))?;
    }
    clock.lap("svg");

    let png = renderer
        .png(&svg, args.scale)
        .context("rasterising the card")?;
    let output = args.output.unwrap_or_else(|| default_output(&input));
    fs::write(&output, png).with_context(|| format!("writing {}", output.display()))?;
    clock.lap("png");

    println!("wrote {}", output.display());
    if args.timing {
        eprint!("{}", clock.report());
    }
    Ok(())
}

/// Wall-clock time per pipeline stage, for `--timing`.
struct Stopwatch {
    start: Instant,
    last: Instant,
    stages: Vec<(&'static str, Duration)>,
}

impl Stopwatch {
    fn start() -> Self {
        let now = Instant::now();
        Self {
            start: now,
            last: now,
            stages: Vec::new(),
        }
    }

    /// Ends the stage `name` now and starts the next one.
    fn lap(&mut self, name: &'static str) {
        let now = Instant::now();
        self.stages.push((name, now - self.last));
        self.last = now;
    }

    fn report(&self) -> String {
        timing_report(&self.stages, self.last - self.start)
    }
}

/// Parses a `--date` value: `YYYY-MM-DD`.
fn parse_date(s: &str) -> Result<Date, String> {
    Date::parse(s, format_description!("[year]-[month]-[day]"))
        .map_err(|e| format!("expected a date as YYYY-MM-DD: {e}"))
}

/// Parses a `--scale` value: a finite number above zero.
fn parse_scale(s: &str) -> Result<f32, String> {
    let scale: f32 = s.parse().map_err(|e| format!("not a number: {e}"))?;
    if scale.is_finite() && scale > 0.0 {
        Ok(scale)
    } else {
        Err(format!("must be a finite number above 0, got {scale}"))
    }
}

/// The PNG next to the input: `runs/berlin.gpx` becomes `runs/berlin.png`.
fn default_output(input: &Path) -> PathBuf {
    input.with_extension("png")
}

/// `--event` if given, else the track's name, else "Run".
fn event_name(arg: Option<String>, track_name: Option<&str>) -> String {
    arg.or_else(|| {
        track_name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
    })
    .unwrap_or_else(|| DEFAULT_EVENT.to_owned())
}

/// `--date` if given, else the UTC date of the first timestamp, else `today`.
fn run_date(
    arg: Option<Date>,
    start: Option<OffsetDateTime>,
    today: impl FnOnce() -> Date,
) -> Date {
    arg.or_else(|| start.map(|t| t.to_offset(UtcOffset::UTC).date()))
        .unwrap_or_else(today)
}

/// One `<stage> <ms> ms` line per stage, then the total.
fn timing_report(stages: &[(&str, Duration)], total: Duration) -> String {
    stages
        .iter()
        .chain([&("total", total)])
        .map(|(stage, d)| format!("{stage} {:.1} ms\n", d.as_secs_f64() * 1000.0))
        .collect()
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
    fn the_output_defaults_to_the_input_with_a_png_extension() {
        assert_eq!(
            default_output(Path::new("runs/berlin.gpx")),
            PathBuf::from("runs/berlin.png")
        );
        assert_eq!(
            default_output(Path::new("berlin")),
            PathBuf::from("berlin.png")
        );
    }

    #[test]
    fn the_event_is_the_argument_else_the_track_name_else_run() {
        assert_eq!(
            event_name(Some("Spreelauf".into()), Some("Morning Run")),
            "Spreelauf"
        );
        assert_eq!(event_name(None, Some("Berlin Marathon")), "Berlin Marathon");
        assert_eq!(event_name(None, Some("  ")), "Run");
        assert_eq!(event_name(None, None), "Run");
    }

    #[test]
    fn the_date_is_the_argument_else_the_utc_start_date_else_today() {
        let today = || date!(2026 - 11 - 08);
        let given = Some(date!(2026 - 05 - 03));
        let start = Some(datetime!(2026-09-27 01:30 +02:00));
        assert_eq!(run_date(given, start, today), date!(2026 - 05 - 03));
        assert_eq!(run_date(None, start, today), date!(2026 - 09 - 26));
        assert_eq!(run_date(None, None, today), date!(2026 - 11 - 08));
    }

    #[test]
    fn the_timing_report_has_a_line_per_stage_and_the_total() {
        let stages = [
            ("parse", Duration::from_micros(1_240)),
            ("png", Duration::from_micros(9_800)),
        ];
        assert_eq!(
            timing_report(&stages, Duration::from_micros(11_460)),
            "parse 1.2 ms\npng 9.8 ms\ntotal 11.5 ms\n"
        );
    }
}
