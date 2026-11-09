//! The router through `oneshot`, without a socket: the shared pipeline,
//! `multipart/form-data` bodies and the answers.

use std::sync::{Arc, OnceLock};

use axum::body::{Body, Bytes};
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use runcard::pipeline::Pipeline;
use runcard::server::{self, Config};
use tower::ServiceExt;

pub const BOUNDARY: &str = "runcard-test-boundary-7MA4YWxkTrZu0gW";

/// One pipeline for all tests: loading the fonts once keeps the suite fast.
pub fn pipeline() -> Arc<Pipeline> {
    static PIPELINE: OnceLock<Arc<Pipeline>> = OnceLock::new();
    PIPELINE
        .get_or_init(|| Arc::new(Pipeline::new().expect("the pipeline loads")))
        .clone()
}

pub fn app() -> Router {
    app_with(Config::default())
}

pub fn app_with(config: Config) -> Router {
    server::router(pipeline(), &config)
}

/// Status, headers and body of the router's answer to `request`.
pub async fn send(app: Router, request: Request<Body>) -> (StatusCode, HeaderMap, Bytes) {
    let response = app.oneshot(request).await.expect("the router answers");
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.expect("a readable body").to_bytes();
    (parts.status, parts.headers, bytes)
}

/// A `multipart/form-data` body: `gpx` as a file part, every other part as text.
pub fn multipart(parts: &[(&str, &str)]) -> Vec<u8> {
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

pub fn post_cards(parts: &[(&str, &str)]) -> Request<Body> {
    Request::post("/cards")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(multipart(parts)))
        .unwrap()
}
