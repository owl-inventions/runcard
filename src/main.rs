//! `runcard <INPUT.gpx> --runner <NAME>`: renders a GPX track as a PNG card.

use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use runcard::pipeline::{parse_date, parse_scale, CardRequest, Pipeline};
use runcard::Error;
use time::Date;

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

/// Renders the card with the shared pipeline and writes the PNG (and the SVG) to disk.
fn run(args: Args) -> Result<()> {
    let input = args.input;
    let file = File::open(&input).with_context(|| format!("reading {}", input.display()))?;
    let pipeline = Pipeline::new().context("loading the card template and fonts")?;
    let request = CardRequest {
        runner: args.runner,
        event: args.event,
        city: args.city,
        date: args.date,
        scale: args.scale,
    };
    let rendered = pipeline
        .render(BufReader::new(file), &request)
        .map_err(|error| with_stage(error, &input))?;

    if let Some(path) = &args.svg {
        fs::write(path, &rendered.svg).with_context(|| format!("writing {}", path.display()))?;
    }
    let output = args.output.unwrap_or_else(|| default_output(&input));
    fs::write(&output, &rendered.png).with_context(|| format!("writing {}", output.display()))?;

    println!("wrote {}", output.display());
    if args.timing {
        let total = rendered.timings.iter().map(|(_, d)| *d).sum();
        eprint!("{}", timing_report(&rendered.timings, total));
    }
    Ok(())
}

/// `error` with the pipeline stage it came from as context.
fn with_stage(error: Error, input: &Path) -> anyhow::Error {
    let stage = match error {
        Error::Io(_) | Error::Gpx(_) | Error::TooFewPoints => {
            format!("reading {}", input.display())
        }
        Error::NoTimestamps
        | Error::MissingTimestamp { .. }
        | Error::TimeNotMonotonic { .. }
        | Error::TooShort => format!("analysing {}", input.display()),
        Error::Template(_) => "filling the card template".to_owned(),
        Error::Svg(_) | Error::Png(_) => "rasterising the card".to_owned(),
    };
    anyhow::Error::new(error).context(stage)
}

/// The PNG next to the input: `runs/berlin.gpx` becomes `runs/berlin.png`.
fn default_output(input: &Path) -> PathBuf {
    input.with_extension("png")
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
