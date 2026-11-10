//! The whole pipeline end to end: a synthetic run through the library to a
//! PNG, and the bundled sample through the `runcard` binary.

mod common;

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use runcard::palette::Palette;
use runcard::render::Renderer;
use runcard::text::TextMeasure;
use runcard::{card, gpx, splits, CardData, Meta, Theme, Variant};
use time::macros::{date, datetime};
use time::OffsetDateTime;

const START: OffsetDateTime = datetime!(2026-04-19 07:30:00 UTC);
const HALF_M: f64 = 21_100.0;
const SCALE: f32 = 2.0;

/// 5:10 per kilometre, except a 4:48 kilometre 21 and a 5:52 kilometre 14.
fn half_pace(km: u32) -> f64 {
    match km {
        14 => 352.0,
        21 => 288.0,
        _ => 310.0,
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

    /// The pixel at card coordinates (`x`, `y`).
    fn at(&self, x: f32, y: f32) -> [u8; 3] {
        self.pixel((x * SCALE) as u32, (y * SCALE) as u32)
    }

    /// Whether any pixel in the card-pixel rectangle `x` by `y` is dark (red below 80).
    fn has_dark_pixels(&self, x: Range<f32>, y: Range<f32>) -> bool {
        let pixels = |range: Range<f32>| (range.start * SCALE) as u32..(range.end * SCALE) as u32;
        pixels(y).any(|py| pixels(x.clone()).any(|px| self.pixel(px, py)[0] < 80))
    }

    /// Whether every pixel in the card-pixel rectangle `x` by `y` is `colour`.
    fn is_all(&self, x: Range<f32>, y: Range<f32>, colour: [u8; 3]) -> bool {
        let pixels = |range: Range<f32>| (range.start * SCALE) as u32..(range.end * SCALE) as u32;
        pixels(y).all(|py| pixels(x.clone()).all(|px| self.pixel(px, py) == colour))
    }
}

fn rgb_of(hex: &str) -> [u8; 3] {
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).expect("a hex colour");
    [channel(1), channel(3), channel(5)]
}

fn assert_colour(actual: [u8; 3], hex: &str, what: &str) {
    let expected = rgb_of(hex);
    assert!(
        actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 3),
        "{what} is {actual:?}, expected {hex}"
    );
}

fn meta(runner: &str, event: &str, city: Option<&str>) -> Meta {
    Meta {
        runner: runner.into(),
        event: event.into(),
        city: city.map(Into::into),
        date: date!(2026 - 04 - 19),
    }
}

/// The ribbon card of the GPX `xml` for `meta`, in light.
fn card_of(xml: &str, name: &str, meta: &Meta) -> CardData {
    card_as(xml, name, meta, Variant::Ribbon)
}

/// The `variant` card of the GPX `xml` for `meta`, in light.
fn card_as(xml: &str, name: &str, meta: &Meta, variant: Variant) -> CardData {
    let path = common::write_fixture(name, xml);
    let analysis = splits::analyze(&gpx::load(&path).unwrap()).unwrap();
    card::build(
        &analysis,
        meta,
        &Palette::for_theme(Theme::Light),
        &TextMeasure::new(),
        variant,
        Theme::Light,
    )
}

#[test]
fn a_synthetic_half_renders_to_the_frame_with_its_texts_legend_and_rule() {
    let xml = common::synthetic_gpx(HALF_M, half_pace, START);
    let data = card_of(
        &xml,
        "render-half.gpx",
        &meta("Mara Lindqvist", "Harbour Half", Some("Lisbon")),
    );
    let fastest = data.stats.fastest.as_ref().expect("a fastest kilometre");
    assert_eq!(
        (fastest.label.as_str(), fastest.pace.as_str()),
        ("fastest km 21", "4:48")
    );
    let renderer = Renderer::new().unwrap();
    let svg = renderer.svg(&data).unwrap();
    let image = Image::decode(&renderer.png(&svg, SCALE).unwrap());
    let light = Theme::Light.tokens();

    assert_eq!((image.width, image.height), (1440, 2000));
    assert_colour(image.at(5.0, 5.0), light.paper, "the paper");
    assert!(
        image.has_dark_pixels(48.0..300.0, 60.0..108.0),
        "the title is not drawn"
    );
    assert!(
        image.has_dark_pixels(48.0..200.0, 128.0..146.0),
        "the byline is not drawn"
    );
    assert!(
        image.has_dark_pixels(48.0..200.0, 830.0..912.0),
        "the time is not drawn"
    );
    assert!(
        image.has_dark_pixels(48.0..100.0, 942.0..954.0),
        "the distance is not drawn"
    );
    assert_colour(
        image.at(49.0, 757.0),
        &data.legend[0],
        "the slow end of the legend",
    );
    assert_colour(
        image.at(206.0, 757.0),
        &data.legend[31],
        "the fast end of the legend",
    );
    assert_colour(image.at(360.0, 740.5), light.rule, "the rule");
    for markup in [
        ">Harbour <tspan",
        ">Half</tspan></text>",
        ">Mara Lindqvist</text>",
        ">LISBON \u{b7} 19 APRIL 2026</text>",
        ">21.10 <tspan",
        ">fastest km 21</text>",
        ">4:48</text>",
    ] {
        assert!(svg.contains(markup), "no {markup:?} in {svg}");
    }
}

#[test]
fn a_run_under_a_kilometre_has_no_fastest_block_and_no_empty_text() {
    let xml = common::synthetic_gpx(600.0, |_| 300.0, START);
    let data = card_of(
        &xml,
        "render-600m.gpx",
        &meta("Mara Lindqvist", "Lunch Run", None),
    );
    assert_eq!(data.stats.fastest, None);
    let svg = Renderer::new().unwrap().svg(&data).unwrap();
    assert!(!svg.contains("fastest km"), "{svg}");
    assert!(
        !svg.contains("\"></text>"),
        "an empty text element in {svg}"
    );
    assert!(svg.contains(">19 APRIL 2026</text>"), "{svg}");
}

#[test]
fn long_names_and_titles_stay_inside_the_content_box() {
    let xml = common::synthetic_gpx(HALF_M, half_pace, START);
    let meta = meta(
        "Maximilian Alexander Konstantin Oberhauser-Lindqvist zu Hohenzollern-Sigmaringen und Waldburg",
        "Schneider Electric Marathon de Paris, presented by the City of Paris and Friends",
        Some("Llanfairpwllgwyngyllgogerychwyrndrobwllllantysiliogogogoch, Isle of Anglesey, Wales, United Kingdom"),
    );
    let data = card_of(&xml, "render-long-names.gpx", &meta);
    let image = Image::decode(&Renderer::new().unwrap().render(&data, SCALE).unwrap());

    assert!(
        image.has_dark_pixels(48.0..300.0, 80.0..108.0),
        "the title is not drawn"
    );
    assert!(
        image.has_dark_pixels(48.0..300.0, 128.0..146.0),
        "the byline is not drawn"
    );
    let paper = rgb_of(Theme::Light.tokens().paper);
    let right_of_the_box = (674.0 * SCALE) as u32..(720.0 * SCALE) as u32;
    for (what, rows) in [
        ("title", 50.0..120.0),
        ("byline", 122.0..154.0),
        ("subline", 162.0..178.0),
    ] {
        let rows = (rows.start * SCALE) as u32..(rows.end * SCALE) as u32;
        assert!(
            rows.clone()
                .all(|y| right_of_the_box.clone().all(|x| image.pixel(x, y) == paper)),
            "the {what} draws right of x 674"
        );
    }
}

#[test]
fn the_runner_is_the_italic_byline_at_26_px_on_the_baseline_146() {
    let xml = common::synthetic_gpx(HALF_M, half_pace, START);
    let data = card_of(
        &xml,
        "render-name.gpx",
        &meta("Mara Lindqvist", "Harbour Half", Some("Lisbon")),
    );
    let svg = Renderer::new().unwrap().svg(&data).unwrap();
    let end = svg
        .find(">Mara Lindqvist</text>")
        .unwrap_or_else(|| panic!("no name in {svg}"));
    let tag = &svg[svg[..end].rfind("<text").expect("a text start tag")..=end];
    for attribute in [
        r#"x="48""#,
        r#"y="146""#,
        r#"font-size="26""#,
        r#"font-family="Instrument Serif" font-style="italic""#,
    ] {
        assert!(tag.contains(attribute), "no {attribute} in {tag}");
    }
    assert_eq!(svg.matches(">Mara Lindqvist<").count(), 1, "{svg}");
}

#[test]
fn the_route_preview_is_inked_flush_right_in_its_box_and_clear_of_the_rule() {
    // A loop taller than the 200 x 56 box: it spans y 750 to 806 and is moved
    // right; it starts on its southern edge, so the start marker reaches y 808.5.
    let xml = common::synthetic_gpx(HALF_M, half_pace, START);
    let meta = meta("Mara Lindqvist", "Harbour Half", Some("Lisbon"));
    let renderer = Renderer::new().unwrap();
    let paper = rgb_of(Theme::Light.tokens().paper);
    for variant in Variant::ALL {
        let name = format!("render-preview-{variant}.gpx");
        let data = card_as(&xml, &name, &meta, variant);
        let image = Image::decode(&renderer.render(&data, SCALE).unwrap());
        let previewed = variant != Variant::Trail;
        assert_eq!(
            image.has_dark_pixels(472.0..673.0, 750.0..807.0),
            previewed,
            "{variant}: the preview box"
        );
        assert_eq!(
            image.has_dark_pixels(670.0..673.0, 750.0..807.0),
            previewed,
            "{variant}: flush right at x 672"
        );
        assert!(
            image.is_all(674.0..720.0, 724.0..810.0, paper),
            "{variant}: ink right of x 674"
        );
        assert!(
            image.is_all(0.0..720.0, 741.0..747.0, paper),
            "{variant}: ink between the rule and the preview"
        );
        assert!(
            image.is_all(472.0..720.0, 809.0..934.0, paper),
            "{variant}: ink between the preview and the stats line"
        );
    }
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
        "Mara Lindqvist".as_ref(),
        "--city".as_ref(),
        "Lisbon".as_ref(),
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
    assert!(svg_text.contains(">Mara Lindqvist</text>"), "{svg_text}");
    let stages: Vec<&str> = stderr
        .lines()
        .map(|line| line.split(' ').next().unwrap_or_default())
        .collect();
    assert_eq!(
        stages,
        ["parse", "analyze", "layout", "svg", "png", "total"],
        "{stderr}"
    );
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
        "Mara Lindqvist".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("reading {}", missing.display())),
        "{stderr}"
    );
}

#[test]
fn a_track_without_timestamps_fails_with_exit_code_1_and_names_the_analysis() {
    let path = common::write_fixture(
        "cli-no-timestamps.gpx",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="runcard-tests" xmlns="http://www.topografix.com/GPX/1/1">
<trk><trkseg><trkpt lat="52.5" lon="13.4"/><trkpt lat="52.51" lon="13.41"/></trkseg></trk>
</gpx>"#,
    );
    let output = runcard(&[
        path.as_os_str(),
        "--runner".as_ref(),
        "Mara Lindqvist".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("analysing {}", path.display())),
        "{stderr}"
    );
    assert!(
        stderr.contains("track has no timestamps, pace cannot be derived"),
        "{stderr}"
    );
}

#[test]
fn an_invalid_gpx_fails_with_exit_code_1_and_names_the_file() {
    let path = common::write_fixture("cli-invalid.gpx", "<gpx version=\"1.1\"><trk><trkseg>");
    let output = runcard(&[
        path.as_os_str(),
        "--runner".as_ref(),
        "Mara Lindqvist".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("reading {}", path.display())),
        "{stderr}"
    );
    assert!(stderr.contains("invalid gpx"), "{stderr}");
}

/// Runs the binary on the sample at scale 1 with `args` and returns the PNG.
fn render_sample(name: &str, args: &[&str]) -> Image {
    let png = output_path(name);
    let sample = sample_gpx();
    let mut all: Vec<&std::ffi::OsStr> = vec![
        sample.as_os_str(),
        "--runner".as_ref(),
        "Mara Lindqvist".as_ref(),
        "--scale".as_ref(),
        "1".as_ref(),
        "-o".as_ref(),
        png.as_os_str(),
    ];
    all.extend(args.iter().map(|arg| std::ffi::OsStr::new(*arg)));
    let output = runcard(&all);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{}\n{stderr}", output.status);
    Image::decode(&std::fs::read(&png).expect("the png is written"))
}

#[test]
fn the_binary_renders_every_variant_it_is_given_on_light_paper() {
    for variant in Variant::ALL {
        let image = render_sample(
            &format!("cli-{variant}.png"),
            &["--variant", variant.name()],
        );
        assert_eq!((image.width, image.height), (720, 1000), "{variant}");
        assert_eq!(
            image.pixel(0, 0),
            rgb_of(Theme::Light.tokens().paper),
            "{variant}"
        );
    }
}

#[test]
fn theme_dark_renders_the_card_on_dark_paper() {
    let image = render_sample("cli-dark.png", &["--theme", "dark"]);
    assert_eq!((image.width, image.height), (720, 1000));
    assert_eq!(image.pixel(0, 0), rgb_of("#0f1115"));
}

/// The exit code and stderr of the binary on the sample with `args`.
fn rejected(args: &[&str]) -> (Option<i32>, String) {
    let sample = sample_gpx();
    let mut all: Vec<&std::ffi::OsStr> = vec![
        sample.as_os_str(),
        "--runner".as_ref(),
        "Mara Lindqvist".as_ref(),
    ];
    all.extend(args.iter().map(|arg| std::ffi::OsStr::new(*arg)));
    let output = runcard(&all);
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn an_unknown_variant_exits_with_code_2_listing_the_variants() {
    let (code, stderr) = rejected(&["--variant", "nope"]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("'nope'"), "{stderr}");
    assert!(stderr.contains("--variant"), "{stderr}");
    assert!(
        stderr.contains("spiral, ribbon, trail, dots, wave, ledger"),
        "{stderr}"
    );
}

#[test]
fn help_lists_the_variants_spiral_first_and_names_it_the_default() {
    let output = runcard(&["--help".as_ref()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    let variant = &stdout[stdout.find("--variant").expect("the --variant flag")..];
    let variant = &variant[..variant.find("--theme").expect("the --theme flag")];
    assert!(variant.contains("[default: spiral]"), "{variant}");
    let listed: Vec<String> = variant
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- "))
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    let expected: Vec<String> = Variant::ALL
        .iter()
        .map(|v| format!("{}: {}", v.name(), v.description()))
        .collect();
    assert_eq!(listed, expected, "{variant}");
}

#[test]
fn without_a_variant_the_binary_renders_the_spiral() {
    let svg_for = |name: &str, args: &[&str]| {
        let svg = output_path(name);
        let png = output_path(&format!("{name}.png"));
        let sample = sample_gpx();
        let mut all: Vec<&std::ffi::OsStr> = vec![
            sample.as_os_str(),
            "--runner".as_ref(),
            "Mara Lindqvist".as_ref(),
            "--scale".as_ref(),
            "0.1".as_ref(),
            "-o".as_ref(),
            png.as_os_str(),
            "--svg".as_ref(),
            svg.as_os_str(),
        ];
        all.extend(args.iter().map(|arg| std::ffi::OsStr::new(*arg)));
        let output = runcard(&all);
        assert!(output.status.success(), "{output:?}");
        std::fs::read_to_string(&svg).expect("the svg is written")
    };
    let default = svg_for("cli-default.svg", &[]);
    assert_eq!(default, svg_for("cli-spiral.svg", &["--variant", "spiral"]));
    assert_ne!(default, svg_for("cli-ribbon.svg", &["--variant", "ribbon"]));
}

#[test]
fn an_unknown_theme_exits_with_code_2_listing_the_themes() {
    let (code, stderr) = rejected(&["--theme", "nope"]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("'nope'"), "{stderr}");
    assert!(stderr.contains("--theme"), "{stderr}");
    assert!(stderr.contains("light, dark"), "{stderr}");
}
