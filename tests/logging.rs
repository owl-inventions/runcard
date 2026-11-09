//! What the router logs, captured as JSON lines: one `request` line per
//! answered request except the health probes, and one `card rendered` line
//! per card.

mod common;

use std::io;
use std::sync::{Arc, Mutex};

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, Request, StatusCode};
use common::http::{app, post_cards, send};
use serde_json::Value;
use tracing::instrument::WithSubscriber;

/// A 5 km synthetic run, five splits.
fn five_k() -> String {
    common::synthetic_gpx(
        5_000.0,
        |_| 300.0,
        time::macros::datetime!(2026-09-27 09:00:00 UTC),
    )
}

/// Everything the log writer received.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The router's answer to `request` and the JSON lines logged while answering.
async fn logged(request: Request<Body>) -> ((StatusCode, HeaderMap, Bytes), Vec<Value>) {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(false)
        .with_writer(move || writer.clone())
        .finish();
    let answer = send(app(), request).with_subscriber(subscriber).await;
    let log = String::from_utf8(captured.0.lock().unwrap().clone()).expect("a utf-8 log");
    let lines = log
        .lines()
        .map(|line| serde_json::from_str(line).expect("a json log line"))
        .collect();
    (answer, lines)
}

/// The lines whose message is `message`.
fn with_message<'a>(lines: &'a [Value], message: &str) -> Vec<&'a Value> {
    lines
        .iter()
        .filter(|line| line["message"] == message)
        .collect()
}

/// The one `request` line among `lines`.
fn request_line(lines: &[Value]) -> &Value {
    let requests = with_message(lines, "request");
    assert_eq!(requests.len(), 1, "not one request line: {lines:?}");
    requests[0]
}

#[tokio::test]
async fn a_card_request_logs_one_request_line_with_the_answers_request_id() {
    let gpx = five_k();
    let request = post_cards(&[
        ("gpx", &gpx),
        ("runner", "Mara Lindqvist"),
        ("format", "svg"),
    ]);
    let ((status, headers, _), lines) = logged(request).await;
    assert_eq!(status, StatusCode::OK);
    let line = request_line(&lines);
    assert_eq!(line["level"], "INFO");
    assert_eq!(line["target"], "runcard::http");
    assert_eq!(line["method"], "POST");
    assert_eq!(line["path"], "/cards");
    assert_eq!(line["status"], 200);
    assert!(line["latency_ms"].is_f64(), "{line}");
    let id = headers["x-request-id"].to_str().unwrap();
    assert_eq!(id.len(), 36, "not a uuid: {id}");
    assert_eq!(line["request_id"], id);
}

#[tokio::test]
async fn health_probes_are_not_logged() {
    let request = Request::get("/health").body(Body::empty()).unwrap();
    let ((status, _, _), lines) = logged(request).await;
    assert_eq!(status, StatusCode::OK);
    assert!(lines.is_empty(), "{lines:?}");
}

#[tokio::test]
async fn variants_unknown_paths_and_wrong_methods_are_logged() {
    for (method, path, status) in [
        ("GET", "/variants", 200),
        ("GET", "/nope", 404),
        ("GET", "/cards", 405),
    ] {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .body(Body::empty())
            .unwrap();
        let ((actual, _, _), lines) = logged(request).await;
        assert_eq!(actual.as_u16(), status, "{method} {path}");
        let line = request_line(&lines);
        assert_eq!(line["method"], method);
        assert_eq!(line["path"], path);
        assert_eq!(line["status"], status);
    }
}

#[tokio::test]
async fn a_bad_request_logs_its_status_and_no_card() {
    let gpx = five_k();
    let ((status, headers, _), lines) = logged(post_cards(&[("gpx", &gpx)])).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(headers.contains_key("x-request-id"), "{headers:?}");
    assert_eq!(request_line(&lines)["status"], 400);
    assert!(
        with_message(&lines, "card rendered").is_empty(),
        "{lines:?}"
    );
}

#[tokio::test]
async fn a_rendered_card_logs_its_look_format_size_and_render_time() {
    let gpx = five_k();
    for (format, scale) in [("svg", "2"), ("png", "0.3")] {
        let request = post_cards(&[
            ("gpx", &gpx),
            ("runner", "Mara Lindqvist"),
            ("variant", "wave"),
            ("theme", "dark"),
            ("format", format),
            ("scale", scale),
        ]);
        let ((status, headers, body), lines) = logged(request).await;
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
        let cards = with_message(&lines, "card rendered");
        assert_eq!(cards.len(), 1, "{lines:?}");
        let card = cards[0];
        assert_eq!(card["level"], "INFO");
        assert_eq!(card["target"], "runcard::cards");
        assert_eq!(card["variant"], "wave");
        assert_eq!(card["theme"], "dark");
        assert_eq!(card["format"], format);
        assert_eq!(card["scale"], scale.parse::<f64>().unwrap(), "{card}");
        assert_eq!(card["splits"], 5);
        assert_eq!(card["bytes"], body.len());
        assert!(
            card["render_ms"].as_f64().is_some_and(|ms| ms > 0.0),
            "{card}"
        );
        let id = headers["x-request-id"].to_str().unwrap();
        assert_eq!(card["request_id"], id, "{card}");
        assert_eq!(request_line(&lines)["request_id"], id);
    }
}

#[tokio::test]
async fn an_incoming_request_id_is_kept_logged_and_echoed() {
    let request = Request::get("/variants")
        .header("x-request-id", "abc")
        .body(Body::empty())
        .unwrap();
    let ((status, headers, _), lines) = logged(request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["x-request-id"], "abc");
    assert_eq!(request_line(&lines)["request_id"], "abc");
}
