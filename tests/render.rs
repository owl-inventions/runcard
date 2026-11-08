//! The whole pipeline end to end: a synthetic marathon through the library to
//! a PNG, and the bundled sample through the `runcard` binary.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use runcard::palette::Palette;
use runcard::render::Renderer;
use runcard::text::TextMeasure;
use runcard::{card, gpx, splits, Meta};
use time::macros::{date, datetime};
use time::OffsetDateTime;

const START: OffsetDateTime = datetime!(2026-09-27 09:00:00 UTC);
const MARATHON_M: f64 = 42_195.0;
const SCALE: f32 = 2.0;
/// Top of the stripe bar and half its height, in card pixels (see the template).
const STRIPE_TOP: f32 = 180.0;
const STRIPE_HALF_HEIGHT: f32 = 190.0;
const BACKGROUND: &str = "#f4efe6";
const FASTEST_COLOUR: &str = "#e8601c";

/// 5:30 per kilometre, except a 4:30 kilometre 30.
fn marathon_pace(km: u32) -> f64 {
    if km == 30 {
        270.0
    } else {
        330.0
    }
}

/// Width, height and RGB bytes of a PNG.
struct Image {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
}

impl Image {
    fn decode(png_bytes: &[u8]) -> Self {
        let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
        let mut reader = decoder.read_info().expect("a valid png header");
        let mut buf = vec![0; reader.output_buffer_size().expect("a sane png size")];
        let info = reader.next_frame(&mut buf).expect("a valid png frame");
        assert_eq!(info.color_type, png::ColorType::Rgb);
        buf.truncate(info.buffer_size());
        Self {
            width: info.width,
            height: info.height,
            rgb: buf,
        }
    }

    fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * self.width + x) * 3) as usize;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }
}

fn rgb_of(hex: &str) -> [u8; 3] {
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).expect("a hex colour");
    [channel(1), channel(3), channel(5)]
}

#[test]
fn a_synthetic_marathon_renders_to_a_card_with_km_30_in_the_fastest_colour() {
    let xml = common::synthetic_gpx(MARATHON_M, marathon_pace, START);
    let path = common::write_fixture("render-marathon.gpx", &xml);
    let analysis = splits::analyze(&gpx::load(&path).unwrap()).unwrap();
    let meta = Meta {
        runner: "Julia Brandt".into(),
        event: "Berlin Marathon".into(),
        city: Some("Berlin".into()),
        date: date!(2026 - 09 - 27),
    };
    let data = card::build(&analysis, &meta, &Palette::new(), &TextMeasure::new());
    let renderer = Renderer::new().unwrap();
    let svg = renderer.svg(&data).unwrap();
    let image = Image::decode(&renderer.png(&svg, SCALE).unwrap());

    assert_eq!((image.width, image.height), (1440, 2000));
    assert_eq!(image.pixel(10, 10), rgb_of(BACKGROUND));
    assert!(
        has_dark_pixels(&image, 40.0..300.0, 650.0..700.0),
        "the runner's name is not drawn"
    );
    assert!(svg.contains("JULIA BRANDT"), "{svg}");
    assert!(svg.contains("FASTEST KM 30"), "{svg}");
    assert!(svg.contains(">KM</text>"), "{svg}");

    let x = data.fastest_x.expect("a fastest marker") * SCALE;
    let y = (STRIPE_TOP + STRIPE_HALF_HEIGHT) * SCALE;
    let actual = image.pixel(x as u32, y as u32);
    let expected = rgb_of(FASTEST_COLOUR);
    for (a, e) in actual.iter().zip(expected) {
        assert!(
            a.abs_diff(e) <= 3,
            "km 30 is {actual:?}, the fastest colour {expected:?}"
        );
    }
}

#[test]
fn a_run_under_a_kilometre_has_no_km_axis_label_and_no_empty_fastest_line() {
    let xml = common::synthetic_gpx(600.0, |_| 300.0, START);
    let path = common::write_fixture("render-600m.gpx", &xml);
    let analysis = splits::analyze(&gpx::load(&path).unwrap()).unwrap();
    let meta = Meta {
        runner: "Julia Brandt".into(),
        event: "Lunch Run".into(),
        city: None,
        date: date!(2026 - 09 - 27),
    };
    let data = card::build(&analysis, &meta, &Palette::new(), &TextMeasure::new());
    assert!(
        data.ticks.is_empty() && data.fastest_line.is_empty(),
        "{data:?}"
    );
    let svg = Renderer::new().unwrap().svg(&data).unwrap();

    assert!(!svg.contains(">KM</text>"), "{svg}");
    assert!(!svg.contains("></text>"), "an empty text element in {svg}");
}

/// Whether any pixel in the card-pixel rectangle `x` by `y` is dark (red below 80).
fn has_dark_pixels(image: &Image, x: std::ops::Range<f32>, y: std::ops::Range<f32>) -> bool {
    let pixels =
        |range: std::ops::Range<f32>| (range.start * SCALE) as u32..(range.end * SCALE) as u32;
    pixels(y).any(|py| pixels(x.clone()).any(|px| image.pixel(px, py)[0] < 80))
}

#[test]
fn long_names_and_titles_stay_inside_the_card() {
    let xml = common::synthetic_gpx(MARATHON_M, marathon_pace, START);
    let path = common::write_fixture("render-long-names.gpx", &xml);
    let analysis = splits::analyze(&gpx::load(&path).unwrap()).unwrap();
    let meta = Meta {
        runner: "Maximilian Oberhauser-Lindqvist".into(),
        event: "Schneider Electric Marathon de Paris".into(),
        city: Some("Paris".into()),
        date: date!(2026 - 04 - 12),
    };
    let data = card::build(&analysis, &meta, &Palette::new(), &TextMeasure::new());
    let image = Image::decode(&Renderer::new().unwrap().render(&data, SCALE).unwrap());

    assert!(
        has_dark_pixels(&image, 40.0..300.0, 650.0..700.0),
        "the name is not drawn"
    );
    assert!(
        !has_dark_pixels(&image, 549.0..561.0, 630.0..710.0),
        "the name runs into the route box"
    );
    assert!(
        has_dark_pixels(&image, 40.0..300.0, 90.0..135.0),
        "the title is not drawn"
    );
    assert!(
        !has_dark_pixels(&image, 690.0..720.0, 75.0..145.0),
        "the title runs off the card"
    );
}

fn sample_gpx() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/sample.gpx")
}

/// A fresh path for a file the binary writes, in this process's test temp dir.
fn output_path(name: &str) -> PathBuf {
    let path = common::temp_dir().join(name);
    let _ = std::fs::remove_file(&path);
    path
}

fn runcard(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_runcard"))
        .args(args)
        .output()
        .expect("the runcard binary runs")
}

#[test]
fn the_binary_renders_the_sample_and_reports_its_timing() {
    let png = output_path("cli-sample.png");
    let svg = output_path("cli-sample.svg");
    let output = runcard(&[
        sample_gpx().as_os_str(),
        "--runner".as_ref(),
        "Julia Brandt".as_ref(),
        "--city".as_ref(),
        "Berlin".as_ref(),
        "--svg".as_ref(),
        svg.as_os_str(),
        "-o".as_ref(),
        png.as_os_str(),
        "--timing".as_ref(),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{}\n{stderr}", output.status);
    assert_eq!(stdout.trim(), format!("wrote {}", png.display()));
    let image = Image::decode(&std::fs::read(&png).expect("the png is written"));
    assert_eq!((image.width, image.height), (1440, 2000));
    let svg_text = std::fs::read_to_string(&svg).expect("the svg is written");
    assert!(svg_text.contains("JULIA BRANDT"), "{svg_text}");
    let stages: Vec<&str> = stderr
        .lines()
        .map(|line| line.split(' ').next().unwrap_or_default())
        .collect();
    assert_eq!(
        stages,
        ["parse", "analyze", "layout", "svg", "png", "total"],
        "{stderr}"
    );
    assert!(stderr.contains("total"), "{stderr}");
    assert!(stdout.contains("wrote"), "{stdout}");
}

#[test]
fn the_binary_requires_a_runner() {
    let output = runcard(&[sample_gpx().as_os_str()]);
    assert!(!output.status.success(), "ran without --runner");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--runner"), "{stderr}");
}

#[test]
fn a_missing_input_fails_with_exit_code_1_and_names_the_file() {
    let missing = output_path("no-such-run.gpx");
    let output = runcard(&[
        missing.as_os_str(),
        "--runner".as_ref(),
        "Julia Brandt".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("reading {}", missing.display())),
        "{stderr}"
    );
}
