//! The HTTP API: the router through `oneshot`, without a socket, and the
//! `runcard-server` binary end to end.

mod common;

use std::sync::{Arc, OnceLock};

use axum::body::{Body, Bytes};
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use runcard::pipeline::Pipeline;
use runcard::server::{self, Config};
use tower::ServiceExt;

/// One pipeline for all tests: loading the fonts once keeps the suite fast.
fn pipeline() -> Arc<Pipeline> {
    static PIPELINE: OnceLock<Arc<Pipeline>> = OnceLock::new();
    PIPELINE
        .get_or_init(|| Arc::new(Pipeline::new().expect("the pipeline loads")))
        .clone()
}

fn app() -> Router {
    app_with(Config::default())
}

fn app_with(config: Config) -> Router {
    server::router(pipeline(), &config)
}

/// Status, headers and body of the router's answer to `request`.
async fn send(app: Router, request: Request<Body>) -> (StatusCode, HeaderMap, Bytes) {
    let response = app.oneshot(request).await.expect("the router answers");
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.expect("a readable body").to_bytes();
    (parts.status, parts.headers, bytes)
}

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

const BOUNDARY: &str = "runcard-test-boundary-7MA4YWxkTrZu0gW";
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

fn start() -> time::OffsetDateTime {
    time::macros::datetime!(2026-09-27 09:00:00 UTC)
}

/// A 5 km synthetic run named "Berlin Marathon", starting on 27 September 2026.
fn five_k() -> String {
    common::synthetic_gpx(5_000.0, |_| 300.0, start())
}

/// A `multipart/form-data` body: `gpx` as a file part, every other part as text.
fn multipart(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut body = String::new();
    for (name, value) in parts {
        body.push_str(&format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\""
        ));
        if *name == "gpx" {
            body.push_str("; filename=\"run.gpx\"\r\nContent-Type: application/gpx+xml");
        }
        body.push_str(&format!("\r\n\r\n{value}\r\n"));
    }
    body.push_str(&format!("--{BOUNDARY}--\r\n"));
    body.into_bytes()
}

fn post_cards(parts: &[(&str, &str)]) -> Request<Body> {
    Request::post("/cards")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(multipart(parts)))
        .unwrap()
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
        ("runner", "Julia Brandt"),
        ("city", "Berlin"),
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
async fn format_svg_returns_the_svg_with_the_runner_in_capitals() {
    let gpx = five_k();
    let request = post_cards(&[("gpx", &gpx), ("runner", "Julia Brandt"), ("format", "svg")]);
    let (status, headers, body) = send(app(), request).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(content_type(&headers), "image/svg+xml; charset=utf-8");
    assert_eq!(
        headers[header::CONTENT_DISPOSITION],
        "inline; filename=\"card.svg\""
    );
    let svg = String::from_utf8(body.to_vec()).expect("utf-8 svg");
    assert!(svg.contains(">JULIA BRANDT</text>"), "{svg}");
}

#[tokio::test]
async fn the_scale_part_sets_the_png_size() {
    let gpx = five_k();
    let request = post_cards(&[("gpx", &gpx), ("runner", "Julia Brandt"), ("scale", "1")]);
    let (status, _, body) = send(app(), request).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(png_size(&body), (720, 1000));
}

#[tokio::test]
async fn the_event_city_and_date_parts_reach_the_card() {
    let gpx = five_k();
    let request = post_cards(&[
        ("gpx", &gpx),
        ("runner", "Julia Brandt"),
        ("event", "Spreelauf"),
        ("city", "Berlin"),
        ("date", "2026-05-03"),
        ("format", "svg"),
    ]);
    let (status, _, body) = send(app(), request).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let svg = String::from_utf8(body.to_vec()).unwrap();
    assert!(svg.contains(">Spreelauf</text>"), "{svg}");
    assert!(svg.contains("BERLIN"), "{svg}");
    assert!(svg.contains("MAY 3, 2026"), "{svg}");
}

#[tokio::test]
async fn empty_optional_parts_count_as_not_given_and_unknown_parts_are_ignored() {
    let gpx = five_k();
    let request = post_cards(&[
        ("gpx", &gpx),
        ("runner", "  Julia Brandt "),
        ("event", ""),
        ("city", " "),
        ("date", ""),
        ("scale", ""),
        ("format", "svg"),
        ("variant", " "),
        ("colour", "pink"),
    ]);
    let (status, _, body) = send(app(), request).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let svg = String::from_utf8(body.to_vec()).unwrap();
    assert!(svg.contains(r##"fill="#f4efe6""##), "not classic: {svg}");
    assert!(svg.contains(">JULIA BRANDT</text>"), "{svg}");
    assert!(svg.contains(">Berlin Marathon</text>"), "{svg}");
    assert!(svg.contains("SEPTEMBER 27, 2026"), "{svg}");
    assert!(svg.contains(">RUN</text>"), "{svg}");
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
        vec![("runner", "Julia Brandt")],
        vec![("gpx", ""), ("runner", "Julia Brandt")],
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
        ("runner", "Julia Brandt"),
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
        let request = post_cards(&[("gpx", &gpx), ("runner", "Julia Brandt"), ("scale", scale)]);
        let message = error_of(app(), request, StatusCode::BAD_REQUEST).await;
        assert!(message.starts_with("invalid scale: "), "{scale}: {message}");
    }
    let request = post_cards(&[("gpx", &gpx), ("runner", "Julia Brandt"), ("scale", "9")]);
    let message = error_of(app(), request, StatusCode::BAD_REQUEST).await;
    assert_eq!(message, "invalid scale: must be at most 8, got 9");
}

#[tokio::test]
async fn a_format_other_than_png_or_svg_is_a_bad_request() {
    let gpx = five_k();
    let request = post_cards(&[
        ("gpx", &gpx),
        ("runner", "Julia Brandt"),
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
        ("runner", "Julia Brandt"),
        ("variant", "noir"),
        ("scale", "1"),
    ]);
    let (status, headers, body) = send(app(), request).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(content_type(&headers), "image/png");
    assert!(body.starts_with(PNG_SIGNATURE));
    assert_eq!(png_size(&body), (720, 1000));

    let request = post_cards(&[
        ("gpx", &gpx),
        ("runner", "Julia Brandt"),
        ("variant", "noir"),
        ("format", "svg"),
    ]);
    let (status, _, body) = send(app(), request).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let svg = String::from_utf8(body.to_vec()).unwrap();
    assert!(svg.contains("feGaussianBlur"), "{svg}");
    assert!(svg.contains(r##"fill="#0f1115""##), "{svg}");
}

#[tokio::test]
async fn an_unknown_variant_is_a_bad_request_listing_the_variants() {
    let gpx = five_k();
    let request = post_cards(&[
        ("gpx", &gpx),
        ("runner", "Julia Brandt"),
        ("variant", "nope"),
    ]);
    let message = error_of(app(), request, StatusCode::BAD_REQUEST).await;
    assert_eq!(
        message,
        r#"invalid variant: expected one of classic, noir, rings, route, bars, mosaic, got "nope""#
    );
}

#[tokio::test]
async fn variants_lists_every_variant_in_display_order() {
    let request = Request::get("/variants").body(Body::empty()).unwrap();
    let (status, headers, body) = send(app(), request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type(&headers), "application/json");
    assert!(
        body.starts_with(br#"[{"name":"classic","description":""#),
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
        ["classic", "noir", "rings", "route", "bars", "mosaic"]
    );
}

#[tokio::test]
async fn a_gpx_without_timestamps_is_a_bad_request_with_the_pipeline_message() {
    let gpx = r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="runcard-tests" xmlns="http://www.topografix.com/GPX/1/1">
<trk><trkseg><trkpt lat="52.5" lon="13.4"/><trkpt lat="52.51" lon="13.41"/></trkseg></trk>
</gpx>"#;
    let request = post_cards(&[("gpx", gpx), ("runner", "Julia Brandt")]);
    let message = error_of(app(), request, StatusCode::BAD_REQUEST).await;
    assert_eq!(message, "track has no timestamps, pace cannot be derived");
}

#[tokio::test]
async fn malformed_gpx_is_a_bad_request() {
    let request = post_cards(&[("gpx", "<gpx><trk>"), ("runner", "Julia Brandt")]);
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
        .body(Body::from(multipart(&[("runner", "Julia Brandt")])))
        .unwrap();
    error_of(app(), without_boundary, StatusCode::BAD_REQUEST).await;

    let mut cut_off = multipart(&[("runner", "Julia Brandt")]);
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
    let request = post_cards(&[("gpx", &gpx), ("runner", "Julia Brandt")]);
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
struct Running(std::process::Child);

#[cfg(unix)]
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Starts the server binary on an ephemeral port with `envs` set and waits
/// for its `listening on` line; returns it with the bound address.
#[cfg(unix)]
fn start_server(envs: &[(&str, String)]) -> (Running, String) {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    let mut command = runcard_server();
    command
        .args(["--bind", "127.0.0.1:0"])
        .env("RUST_LOG", "info")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for (name, value) in envs {
        command.env(name, value);
    }
    let mut server = Running(command.spawn().expect("the server binary starts"));
    let mut stderr = BufReader::new(server.0.stderr.take().unwrap());
    let mut line = String::new();
    let addr = loop {
        line.clear();
        if stderr.read_line(&mut line).unwrap() == 0 {
            panic!("the server exited without listening");
        }
        if let Some((_, addr)) = line.split_once("listening on ") {
            break addr.trim().to_owned();
        }
    };
    assert!(
        !line.contains('\x1b'),
        "colour codes in a piped log: {line:?}"
    );
    (server, addr)
}

/// Sends `signal` (`INT` or `TERM`) to the server and waits for it to exit.
#[cfg(unix)]
fn stop_server(server: &mut Running, signal: &str) -> std::process::ExitStatus {
    let killed = std::process::Command::new("kill")
        .args([&format!("-{signal}"), &server.0.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    server.0.wait().unwrap()
}

#[cfg(unix)]
#[test]
fn the_binary_shuts_down_cleanly_on_sigterm() {
    let (mut server, _) = start_server(&[]);
    let status = stop_server(&mut server, "TERM");
    assert_eq!(status.code(), Some(0), "{status}");
}

#[cfg(unix)]
#[test]
fn the_binary_serves_health_to_the_allowed_origins_and_shuts_down_on_ctrl_c() {
    use std::io::{Read, Write};

    let (mut server, addr) = start_server(&[(
        "RUNCARD_ALLOW_ORIGIN",
        format!("https://other.example, {ORIGIN}"),
    )]);

    let mut stream = std::net::TcpStream::connect(&addr).unwrap();
    stream
        .write_all(
            format!(
                "GET /health HTTP/1.1\r\nHost: localhost\r\nOrigin: {ORIGIN}\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    assert!(answer.starts_with("HTTP/1.1 200 OK"), "{answer}");
    assert!(
        answer.contains(&format!("access-control-allow-origin: {ORIGIN}\r\n")),
        "{answer}"
    );
    assert!(answer.ends_with(r#"{"status":"ok"}"#), "{answer}");

    let status = stop_server(&mut server, "INT");
    assert!(status.success(), "{status}");
}
