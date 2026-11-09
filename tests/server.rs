//! The HTTP API: the router through `oneshot`, without a socket, and the
//! `runcard-server` binary end to end.

mod common;

use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::Router;
use common::http::{app, app_with, multipart, post_cards, send, BOUNDARY};
use runcard::server::Config;

fn content_type(headers: &HeaderMap) -> &str {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
}

fn json(body: &[u8]) -> serde_json::Value {
    serde_json::from_slice(body).expect("a json body")
}

#[tokio::test]
async fn health_answers_ok_as_json() {
    let request = Request::get("/health").body(Body::empty()).unwrap();
    let (status, headers, body) = send(app(), request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type(&headers), "application/json");
    assert_eq!(json(&body), serde_json::json!({ "status": "ok" }));
}

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

fn start() -> time::OffsetDateTime {
    time::macros::datetime!(2026-09-27 09:00:00 UTC)
}

/// A 5 km synthetic run named "Harbour Half", starting on 27 September 2026.
fn five_k() -> String {
    common::synthetic_gpx(5_000.0, |_| 300.0, start())
}

/// Width and height of a PNG.
fn png_size(png_bytes: &[u8]) -> (u32, u32) {
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let info = decoder.read_info().expect("a valid png header");
    (info.info().width, info.info().height)
}

#[tokio::test]
async fn a_gpx_upload_comes_back_as_a_png_card_at_the_default_scale() {
    let gpx = five_k();
    let request = post_cards(&[
        ("gpx", &gpx),
        ("runner", "Mara Lindqvist"),
        ("city", "Lisbon"),
    ]);
    let (status, headers, body) = send(app(), request).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(content_type(&headers), "image/png");
    assert_eq!(
        headers[header::CONTENT_DISPOSITION],
        "inline; filename=\"card.png\""
    );
    assert!(body.starts_with(PNG_SIGNATURE));
    assert_eq!(png_size(&body), (1440, 2000));
}

#[tokio::test]
async fn format_svg_returns_the_svg_with_the_runner_as_typed() {
    let gpx = five_k();
    let request = post_cards(&[
        ("gpx", &gpx),
        ("runner", "Mara Lindqvist"),
        ("format", "svg"),
    ]);
    let (status, headers, body) = send(app(), request).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(content_type(&headers), "image/svg+xml; charset=utf-8");
    assert_eq!(
        headers[header::CONTENT_DISPOSITION],
        "inline; filename=\"card.svg\""
    );
    let svg = String::from_utf8(body.to_vec()).expect("utf-8 svg");
    assert!(svg.contains(">Mara Lindqvist</text>"), "{svg}");
}

#[tokio::test]
async fn the_scale_part_sets_the_png_size() {
    let gpx = five_k();
    let request = post_cards(&[("gpx", &gpx), ("runner", "Mara Lindqvist"), ("scale", "1")]);
    let (status, _, body) = send(app(), request).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(png_size(&body), (720, 1000));
}

#[tokio::test]
async fn the_event_city_and_date_parts_reach_the_card() {
    let gpx = five_k();
    let request = post_cards(&[
        ("gpx", &gpx),
        ("runner", "Mara Lindqvist"),
        ("event", "Spreelauf"),
        ("city", "Lisbon"),
        ("date", "2026-05-03"),
        ("format", "svg"),
    ]);
    let (status, _, body) = send(app(), request).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let svg = String::from_utf8(body.to_vec()).unwrap();
    assert!(svg.contains(">Spreelauf</text>"), "{svg}");
    assert!(svg.contains(">Lisbon \u{b7} 3 May 2026</text>"), "{svg}");
}

#[tokio::test]
async fn empty_optional_parts_count_as_not_given_and_unknown_parts_are_ignored() {
    let gpx = five_k();
    let request = post_cards(&[
        ("gpx", &gpx),
        ("runner", "  Mara Lindqvist "),
        ("event", ""),
        ("city", " "),
        ("date", ""),
        ("scale", ""),
        ("format", "svg"),
        ("variant", " "),
        ("theme", ""),
        ("colour", "pink"),
    ]);
    let (status, _, body) = send(app(), request).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let svg = String::from_utf8(body.to_vec()).unwrap();
    let ribbon_light = svg_card(&[("variant", "ribbon"), ("theme", "light")]).await;
    assert_eq!(svg, ribbon_light, "not the default ribbon on light paper");
    assert!(svg.contains(r##"fill="#f6f6f4""##), "not light: {svg}");
    assert!(svg.contains(">Mara Lindqvist</text>"), "{svg}");
    assert!(svg.contains(">Harbour Half</text>"), "{svg}");
    assert!(svg.contains(">27 September 2026</text>"), "{svg}");
}

/// The SVG card for the five kilometres with `parts` besides gpx, runner and format.
async fn svg_card(parts: &[(&str, &str)]) -> String {
    let gpx = five_k();
    let mut all = vec![
        ("gpx", gpx.as_str()),
        ("runner", "Mara Lindqvist"),
        ("format", "svg"),
    ];
    all.extend_from_slice(parts);
    let (status, _, body) = send(app(), post_cards(&all)).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    String::from_utf8(body.to_vec()).expect("utf-8 svg")
}

/// The RGB colour of the top-left pixel of a PNG.
fn top_left(png_bytes: &[u8]) -> [u8; 3] {
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = decoder.read_info().expect("a valid png header");
    let mut buf = vec![0; reader.output_buffer_size().expect("a sane png size")];
    reader.next_frame(&mut buf).expect("a valid png frame");
    [buf[0], buf[1], buf[2]]
}

/// Asserts a JSON error answer with `status` and returns its message.
async fn error_of(app: Router, request: Request<Body>, status: StatusCode) -> String {
    let (actual, headers, body) = send(app, request).await;
    assert_eq!(actual, status, "{}", String::from_utf8_lossy(&body));
    assert_eq!(content_type(&headers), "application/json");
    json(&body)["error"]
        .as_str()
        .expect("an error message")
        .to_owned()
}

#[tokio::test]
async fn a_missing_or_empty_gpx_part_is_a_bad_request() {
    for parts in [
        vec![("runner", "Mara Lindqvist")],
        vec![("gpx", ""), ("runner", "Mara Lindqvist")],
    ] {
        let message = error_of(app(), post_cards(&parts), StatusCode::BAD_REQUEST).await;
        assert_eq!(message, "gpx is missing or empty", "{parts:?}");
    }
}

#[tokio::test]
async fn a_missing_or_blank_runner_part_is_a_bad_request() {
    let gpx = five_k();
    for parts in [
        vec![("gpx", gpx.as_str())],
        vec![("gpx", &gpx), ("runner", "  ")],
    ] {
        let message = error_of(app(), post_cards(&parts), StatusCode::BAD_REQUEST).await;
        assert_eq!(message, "runner is missing or empty");
    }
}

#[tokio::test]
async fn an_invalid_date_is_a_bad_request() {
    let gpx = five_k();
    let request = post_cards(&[
        ("gpx", &gpx),
        ("runner", "Mara Lindqvist"),
        ("date", "2026-13-01"),
    ]);
    let message = error_of(app(), request, StatusCode::BAD_REQUEST).await;
    assert!(
        message.starts_with("invalid date: expected a date as YYYY-MM-DD"),
        "{message}"
    );
}

#[tokio::test]
async fn a_scale_that_is_not_above_0_and_at_most_8_is_a_bad_request() {
    let gpx = five_k();
    for scale in ["0", "9", "-1", "abc", "NaN", "inf"] {
        let request = post_cards(&[
            ("gpx", &gpx),
            ("runner", "Mara Lindqvist"),
            ("scale", scale),
        ]);
        let message = error_of(app(), request, StatusCode::BAD_REQUEST).await;
        assert!(message.starts_with("invalid scale: "), "{scale}: {message}");
    }
    let request = post_cards(&[("gpx", &gpx), ("runner", "Mara Lindqvist"), ("scale", "9")]);
    let message = error_of(app(), request, StatusCode::BAD_REQUEST).await;
    assert_eq!(message, "invalid scale: must be at most 8, got 9");
}

#[tokio::test]
async fn a_format_other_than_png_or_svg_is_a_bad_request() {
    let gpx = five_k();
    let request = post_cards(&[
        ("gpx", &gpx),
        ("runner", "Mara Lindqvist"),
        ("format", "jpeg"),
    ]);
    let message = error_of(app(), request, StatusCode::BAD_REQUEST).await;
    assert_eq!(message, "invalid format: expected png or svg, got \"jpeg\"");
}

#[tokio::test]
async fn the_variant_part_picks_the_card_look() {
    let gpx = five_k();
    let request = post_cards(&[
        ("gpx", &gpx),
        ("runner", "Mara Lindqvist"),
        ("variant", "spiral"),
        ("scale", "1"),
    ]);
    let (status, headers, body) = send(app(), request).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(content_type(&headers), "image/png");
    assert!(body.starts_with(PNG_SIGNATURE));
    assert_eq!(png_size(&body), (720, 1000));

    let ribbon = svg_card(&[("variant", "ribbon")]).await;
    for variant in ["spiral", "trail", "dots", "wave", "ledger"] {
        let svg = svg_card(&[("variant", variant)]).await;
        assert_ne!(svg, ribbon, "{variant} renders the ribbon");
    }
}

#[tokio::test]
async fn the_theme_part_dark_answers_a_card_on_dark_paper() {
    let gpx = five_k();
    let request = post_cards(&[
        ("gpx", &gpx),
        ("runner", "Mara Lindqvist"),
        ("theme", "dark"),
        ("scale", "1"),
    ]);
    let (status, headers, body) = send(app(), request).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(content_type(&headers), "image/png");
    assert_eq!(png_size(&body), (720, 1000));
    assert_eq!(top_left(&body), [0x0f, 0x11, 0x15]);

    let svg = svg_card(&[("theme", "dark")]).await;
    assert!(svg.contains(r##"fill="#0f1115""##), "{svg}");
    assert!(svg.contains("feGaussianBlur"), "{svg}");
    let light = svg_card(&[("theme", "light")]).await;
    assert!(!light.contains("feGaussianBlur"), "{light}");
}

#[tokio::test]
async fn an_unknown_theme_is_a_bad_request_naming_both_themes() {
    let gpx = five_k();
    let request = post_cards(&[
        ("gpx", &gpx),
        ("runner", "Mara Lindqvist"),
        ("theme", "nope"),
    ]);
    let message = error_of(app(), request, StatusCode::BAD_REQUEST).await;
    assert_eq!(
        message,
        r#"invalid theme: expected light or dark, got "nope""#
    );
}

#[tokio::test]
async fn an_unknown_variant_is_a_bad_request_listing_the_variants() {
    let gpx = five_k();
    let request = post_cards(&[
        ("gpx", &gpx),
        ("runner", "Mara Lindqvist"),
        ("variant", "nope"),
    ]);
    let message = error_of(app(), request, StatusCode::BAD_REQUEST).await;
    assert_eq!(
        message,
        r#"invalid variant: expected one of ribbon, spiral, trail, dots, wave, ledger, got "nope""#
    );
}

#[tokio::test]
async fn variants_lists_every_variant_in_display_order() {
    let request = Request::get("/variants").body(Body::empty()).unwrap();
    let (status, headers, body) = send(app(), request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type(&headers), "application/json");
    assert!(
        body.starts_with(br#"[{"name":"ribbon","description":""#),
        "{}",
        String::from_utf8_lossy(&body)
    );
    let expected: Vec<serde_json::Value> = runcard::Variant::ALL
        .iter()
        .map(|v| serde_json::json!({ "name": v.name(), "description": v.description() }))
        .collect();
    assert_eq!(json(&body), serde_json::Value::Array(expected));
    let names: Vec<String> = json(&body)
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        names,
        ["ribbon", "spiral", "trail", "dots", "wave", "ledger"]
    );
}

#[tokio::test]
async fn a_gpx_without_timestamps_is_a_bad_request_with_the_pipeline_message() {
    let gpx = r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="runcard-tests" xmlns="http://www.topografix.com/GPX/1/1">
<trk><trkseg><trkpt lat="52.5" lon="13.4"/><trkpt lat="52.51" lon="13.41"/></trkseg></trk>
</gpx>"#;
    let request = post_cards(&[("gpx", gpx), ("runner", "Mara Lindqvist")]);
    let message = error_of(app(), request, StatusCode::BAD_REQUEST).await;
    assert_eq!(message, "track has no timestamps, pace cannot be derived");
}

#[tokio::test]
async fn malformed_gpx_is_a_bad_request() {
    let request = post_cards(&[("gpx", "<gpx><trk>"), ("runner", "Mara Lindqvist")]);
    let message = error_of(app(), request, StatusCode::BAD_REQUEST).await;
    assert!(message.starts_with("invalid gpx: "), "{message}");
}

#[tokio::test]
async fn a_body_that_is_not_multipart_is_an_unsupported_media_type() {
    for content_type in [Some("application/json"), None] {
        let mut request = Request::post("/cards");
        if let Some(content_type) = content_type {
            request = request.header(header::CONTENT_TYPE, content_type);
        }
        let request = request.body(Body::from(r#"{"runner":"Julia"}"#)).unwrap();
        let message = error_of(app(), request, StatusCode::UNSUPPORTED_MEDIA_TYPE).await;
        assert_eq!(message, "expected a multipart/form-data body");
    }
}

#[tokio::test]
async fn a_malformed_multipart_body_is_a_bad_request() {
    let without_boundary = Request::post("/cards")
        .header(header::CONTENT_TYPE, "multipart/form-data")
        .body(Body::from(multipart(&[("runner", "Mara Lindqvist")])))
        .unwrap();
    error_of(app(), without_boundary, StatusCode::BAD_REQUEST).await;

    let mut cut_off = multipart(&[("runner", "Mara Lindqvist")]);
    cut_off.truncate(cut_off.len() - 12);
    let cut_off = Request::post("/cards")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(cut_off))
        .unwrap();
    error_of(app(), cut_off, StatusCode::BAD_REQUEST).await;
}

#[tokio::test]
async fn a_body_above_the_limit_is_too_large() {
    let app = app_with(Config {
        max_body: 1024,
        ..Config::default()
    });
    let gpx = five_k();
    let request = post_cards(&[("gpx", &gpx), ("runner", "Mara Lindqvist")]);
    error_of(app, request, StatusCode::PAYLOAD_TOO_LARGE).await;
}

#[test]
fn the_default_body_limit_is_20_mib() {
    assert_eq!(Config::default().max_body, 20 * 1024 * 1024);
}

const ORIGIN: &str = "https://cards.example";

fn allowing(origins: &[&str]) -> Router {
    app_with(Config {
        allow_origins: origins
            .iter()
            .map(|origin| origin.parse().expect("a valid origin"))
            .collect(),
        ..Config::default()
    })
}

fn health_from(origin: &str) -> Request<Body> {
    Request::get("/health")
        .header(header::ORIGIN, origin)
        .body(Body::empty())
        .unwrap()
}

fn preflight(origin: &str) -> Request<Body> {
    Request::options("/cards")
        .header(header::ORIGIN, origin)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn without_allowed_origins_there_are_no_cors_headers() {
    let (_, headers, _) = send(app(), health_from(ORIGIN)).await;
    assert!(
        !headers.contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN),
        "{headers:?}"
    );
}

#[tokio::test]
async fn an_allowed_origin_gets_a_preflight_answer_and_the_allow_origin_header() {
    let (status, headers, _) = send(allowing(&[ORIGIN]), preflight(ORIGIN)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], ORIGIN);
    let methods = headers[header::ACCESS_CONTROL_ALLOW_METHODS]
        .to_str()
        .unwrap();
    assert!(methods.contains("POST"), "{methods}");

    let (status, headers, _) = send(allowing(&[ORIGIN]), health_from(ORIGIN)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], ORIGIN);
}

#[tokio::test]
async fn an_origin_not_on_the_list_gets_no_allow_origin_header() {
    let app = allowing(&[ORIGIN, "https://other.example"]);
    let (_, headers, _) = send(app, health_from("https://evil.example")).await;
    assert!(
        !headers.contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN),
        "{headers:?}"
    );
}

#[tokio::test]
async fn an_allowed_origin_may_read_the_request_id() {
    let (_, headers, _) = send(allowing(&[ORIGIN]), health_from(ORIGIN)).await;
    let exposed = headers[header::ACCESS_CONTROL_EXPOSE_HEADERS]
        .to_str()
        .unwrap();
    assert!(
        exposed
            .split(',')
            .any(|name| name.trim().eq_ignore_ascii_case("x-request-id")),
        "{exposed}"
    );
    assert!(headers.contains_key("x-request-id"), "{headers:?}");

    let (_, headers, _) = send(allowing(&["*"]), health_from(ORIGIN)).await;
    assert_eq!(headers[header::ACCESS_CONTROL_EXPOSE_HEADERS], "*");
}

#[tokio::test]
async fn a_star_allows_any_origin() {
    let (_, headers, _) = send(allowing(&["*"]), health_from("https://anywhere.example")).await;
    assert_eq!(headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
}

fn runcard_server() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_runcard-server"))
}

#[test]
fn the_binary_exits_with_1_when_the_address_is_taken() {
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = taken.local_addr().unwrap().to_string();
    let output = runcard_server()
        .env("RUNCARD_BIND", &addr)
        .output()
        .expect("the server binary runs");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(&format!("binding {addr}")), "{stderr}");
}

#[test]
fn the_binary_rejects_a_body_limit_of_0() {
    let output = runcard_server()
        .args(["--max-body-mib", "0"])
        .output()
        .expect("the server binary runs");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--max-body-mib"), "{stderr}");
}

/// A started server, killed when the test ends, also when an assertion fails.
#[cfg(unix)]
struct Running {
    child: std::process::Child,
    /// The address it listens on.
    addr: String,
    /// Its stderr lines up to and including the `listening on` line.
    log: Vec<String>,
    stderr: std::io::BufReader<std::process::ChildStderr>,
}

#[cfg(unix)]
impl Running {
    /// The stderr lines after the `listening on` line, up to the end; call it
    /// once the server has stopped.
    fn rest_of_log(&mut self) -> Vec<String> {
        use std::io::BufRead;

        (&mut self.stderr)
            .lines()
            .map(|line| line.expect("a utf-8 log line"))
            .collect()
    }
}

#[cfg(unix)]
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Starts the server binary on an ephemeral port with `args` and `envs` and
/// waits for its `listening on` line.
#[cfg(unix)]
fn start_server(args: &[&str], envs: &[(&str, String)]) -> Running {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    let mut command = runcard_server();
    command
        .args(["--bind", "127.0.0.1:0"])
        .args(args)
        .env("RUST_LOG", "info")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for (name, value) in envs {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("the server binary starts");
    let mut stderr = BufReader::new(child.stderr.take().unwrap());
    let mut log = Vec::new();
    let addr = loop {
        let mut line = String::new();
        if stderr.read_line(&mut line).unwrap() == 0 {
            panic!("the server exited without listening: {log:?}");
        }
        let addr = listening_addr(&line);
        log.push(line.trim_end().to_owned());
        if let Some(addr) = addr {
            break addr;
        }
    };
    let listening = log.last().unwrap();
    assert!(
        !listening.contains('\x1b'),
        "colour codes in a piped log: {listening:?}"
    );
    Running {
        child,
        addr,
        log,
        stderr,
    }
}

/// The address of a `listening on <ADDR>` line, written as text or JSON.
#[cfg(unix)]
fn listening_addr(line: &str) -> Option<String> {
    let message = match serde_json::from_str::<serde_json::Value>(line) {
        Ok(json) => json["message"].as_str()?.to_owned(),
        Err(_) => line.to_owned(),
    };
    let (_, addr) = message.split_once("listening on ")?;
    Some(addr.trim().to_owned())
}

/// Sends `signal` (`INT` or `TERM`) to the server and waits for it to exit.
#[cfg(unix)]
fn stop_server(server: &mut Running, signal: &str) -> std::process::ExitStatus {
    let killed = std::process::Command::new("kill")
        .args([&format!("-{signal}"), &server.child.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    server.child.wait().unwrap()
}

/// The raw HTTP/1.1 answer of the server at `addr` to `GET <path>` with `headers`.
#[cfg(unix)]
fn get(addr: &str, path: &str, headers: &[(&str, &str)]) -> String {
    use std::io::{Read, Write};

    let mut request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("Connection: close\r\n\r\n");
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    answer
}

/// The value of the header `name` (lowercase) in a raw HTTP/1.1 answer.
#[cfg(unix)]
fn header_of<'a>(answer: &'a str, name: &str) -> Option<&'a str> {
    answer
        .lines()
        .find_map(|line| line.strip_prefix(name)?.strip_prefix(": "))
}

#[cfg(unix)]
#[test]
fn the_binary_shuts_down_cleanly_on_sigterm() {
    let mut server = start_server(&[], &[]);
    let status = stop_server(&mut server, "TERM");
    assert_eq!(status.code(), Some(0), "{status}");
}

#[cfg(unix)]
#[test]
fn the_binary_logs_json_lines_with_log_format_json() {
    let mut server = start_server(&["--log-format", "json"], &[]);
    assert_eq!(
        server.log.len(),
        1,
        "lines before listening: {:?}",
        server.log
    );
    let listening: serde_json::Value =
        serde_json::from_str(&server.log[0]).expect("a json listening line");
    assert_eq!(listening["level"], "INFO");
    assert_eq!(
        listening["message"],
        format!("listening on {}", server.addr)
    );
    assert!(listening["timestamp"].is_string(), "{listening}");

    let answer = get(&server.addr, "/variants", &[]);
    assert!(answer.starts_with("HTTP/1.1 200 OK"), "{answer}");
    let request_id = header_of(&answer, "x-request-id").expect("an x-request-id header");
    get(&server.addr, "/health", &[]);

    let status = stop_server(&mut server, "TERM");
    assert!(status.success(), "{status}");
    let rest: Vec<serde_json::Value> = server
        .rest_of_log()
        .iter()
        .map(|line| serde_json::from_str(line).expect("a json log line"))
        .collect();
    let requests: Vec<_> = rest
        .iter()
        .filter(|line| line["message"] == "request")
        .collect();
    assert_eq!(requests.len(), 1, "{rest:?}");
    assert_eq!(requests[0]["path"], "/variants");
    assert_eq!(requests[0]["method"], "GET");
    assert_eq!(requests[0]["status"], 200);
    assert_eq!(requests[0]["request_id"], request_id);
    assert!(requests[0].get("span").is_none(), "{}", requests[0]);
    assert_eq!(rest.last().unwrap()["message"], "shutting down", "{rest:?}");
}

#[cfg(unix)]
#[test]
fn the_binary_logs_json_into_a_pipe_by_default() {
    let mut server = start_server(&[], &[]);
    let listening: serde_json::Value =
        serde_json::from_str(&server.log[0]).expect("a json listening line");
    assert_eq!(listening["level"], "INFO");
    assert!(stop_server(&mut server, "TERM").success());
}

#[cfg(unix)]
#[test]
fn the_binary_logs_text_into_a_pipe_when_asked() {
    let mut server = start_server(&[], &[("RUNCARD_LOG_FORMAT", "text".into())]);
    let listening = &server.log[0];
    assert!(
        serde_json::from_str::<serde_json::Value>(listening).is_err(),
        "{listening}"
    );
    assert!(listening.contains(" INFO "), "{listening}");
    assert!(
        listening.ends_with(&format!("listening on {}", server.addr)),
        "{listening}"
    );
    assert!(stop_server(&mut server, "TERM").success());
}

#[cfg(unix)]
#[test]
fn the_binary_serves_health_to_the_allowed_origins_and_shuts_down_on_ctrl_c() {
    let mut server = start_server(
        &[],
        &[(
            "RUNCARD_ALLOW_ORIGIN",
            format!("https://other.example, {ORIGIN}"),
        )],
    );

    let answer = get(&server.addr, "/health", &[("Origin", ORIGIN)]);
    assert!(answer.starts_with("HTTP/1.1 200 OK"), "{answer}");
    assert!(
        answer.contains(&format!("access-control-allow-origin: {ORIGIN}\r\n")),
        "{answer}"
    );
    assert!(answer.ends_with(r#"{"status":"ok"}"#), "{answer}");

    let status = stop_server(&mut server, "INT");
    assert!(status.success(), "{status}");
}
