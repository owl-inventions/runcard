//! The HTTP API: `POST /cards` renders an uploaded GPX track as a card,
//! `GET /variants` lists the card looks, `GET /health` answers that the
//! server is up.
//!
//! Every answer carries an `x-request-id`, the client's own or a new UUID.
//! Each answer except `/health` is logged as one `request` event (target
//! `runcard::http`), and each rendered card as one `card rendered` event
//! (target `runcard::cards`); both, and the errors of a failed render, carry
//! the request id.

use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::multipart::{Multipart, MultipartError, MultipartRejection};
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{header, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::task::JoinError;
use tower_http::cors::{Any, CorsLayer};
use tower_http::request_id::{
    MakeRequestUuid, PropagateRequestIdLayer, RequestId, SetRequestIdLayer,
};

use crate::error::Error;
use crate::pipeline::{parse_date, parse_scale, CardRequest, Pipeline};
use crate::variant::Variant;

/// The request body limit unless configured otherwise: 20 MiB.
pub const DEFAULT_MAX_BODY: usize = 20 * 1024 * 1024;
/// The header that carries the request id.
const X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");
/// The largest raster scale a request may ask for (a 5760x8000 PNG).
pub const MAX_SCALE: f32 = 8.0;
/// The raster scale when the request names none (a 1440x2000 PNG).
const DEFAULT_SCALE: f32 = 2.0;

/// How the router is set up.
#[derive(Debug, Clone)]
pub struct Config {
    /// The largest accepted request body, in bytes.
    pub max_body: usize,
    /// Origins a browser may call the API from; `*` allows any. Without
    /// origins the API sends no CORS headers.
    pub allow_origins: Vec<HeaderValue>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_body: DEFAULT_MAX_BODY,
            allow_origins: Vec::new(),
        }
    }
}

/// The API's routes and layers around the shared `pipeline`.
pub fn router(pipeline: Arc<Pipeline>, config: &Config) -> Router {
    let router = Router::new()
        .route("/variants", get(variants))
        .route("/cards", post(create_card))
        // Logs the routes above and the 404 and 405 answers; `/health`,
        // added after it, stays out of the log the probes would fill.
        .layer(middleware::from_fn(log_request))
        .route("/health", get(health))
        .with_state(pipeline)
        .layer(DefaultBodyLimit::max(config.max_body));
    let router = match cors(&config.allow_origins) {
        Some(cors) => router.layer(cors),
        None => router,
    };
    // The outer layer keeps the client's `x-request-id` or sets a new UUID,
    // the inner one copies it onto the answer.
    router
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}

/// Logs the answer to `request` as one `request` event: method, path,
/// status, latency in milliseconds and the request id.
async fn log_request(request: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let request_id = id_text(request.extensions().get::<RequestId>());
    let response = next.run(request).await;
    tracing::info!(
        target: "runcard::http",
        method = method.as_str(),
        path,
        status = response.status().as_u16(),
        latency_ms = millis(started.elapsed()),
        request_id,
        "request"
    );
    response
}

/// The request id as text; empty without one or when it is not visible ASCII.
fn id_text(id: Option<&RequestId>) -> String {
    id.and_then(|id| id.header_value().to_str().ok())
        .unwrap_or_default()
        .to_owned()
}

/// `duration` in milliseconds, rounded to a tenth.
fn millis(duration: Duration) -> f64 {
    (duration.as_secs_f64() * 10_000.0).round() / 10.0
}

/// CORS for the listed origins, any origin if one of them is `*`, none without
/// origins. The browser may read `x-request-id` (the permissive `*` exposes
/// every header).
fn cors(origins: &[HeaderValue]) -> Option<CorsLayer> {
    if origins.is_empty() {
        None
    } else if origins.iter().any(|origin| origin == "*") {
        Some(CorsLayer::permissive())
    } else {
        Some(
            CorsLayer::new()
                .allow_methods([Method::GET, Method::POST])
                .allow_headers(Any)
                .allow_origin(origins.to_vec())
                .expose_headers([X_REQUEST_ID]),
        )
    }
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// A card look as `GET /variants` lists it.
#[derive(Debug, Serialize)]
struct VariantInfo {
    name: &'static str,
    description: &'static str,
}

/// Every variant in display order, the default first.
async fn variants() -> Json<Vec<VariantInfo>> {
    Json(
        Variant::ALL
            .iter()
            .map(|variant| VariantInfo {
                name: variant.name(),
                description: variant.description(),
            })
            .collect(),
    )
}

/// Renders the uploaded GPX track as a card, off the async runtime.
async fn create_card(
    State(pipeline): State<Arc<Pipeline>>,
    request_id: Option<Extension<RequestId>>,
    multipart: Result<Option<Multipart>, MultipartRejection>,
) -> Result<Card, ApiError> {
    let request_id = id_text(request_id.as_deref());
    let multipart = multipart?.ok_or_else(|| {
        ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "expected a multipart/form-data body",
        )
    })?;
    let (gpx, request, format) = Form::read(multipart).await?.into_request()?;
    let (variant, theme, scale) = (request.variant, request.theme, request.scale);
    let rendered = tokio::task::spawn_blocking(move || format.render(&pipeline, &gpx, &request))
        .await
        .map_err(|error| ApiError::from_task(&error, &request_id))?
        .map_err(|error| ApiError::from_render(error, &request_id))?;
    tracing::info!(
        target: "runcard::cards",
        variant = variant.name(),
        theme = theme.name(),
        format = format.name(),
        scale = widen(scale),
        splits = rendered.splits,
        bytes = rendered.card.len(),
        render_ms = millis(rendered.time),
        request_id,
        "card rendered"
    );
    Ok(rendered.card)
}

/// `scale` as the `f64` with the same digits: `0.3` stays `0.3` in the log
/// instead of becoming `0.30000001192092896`.
fn widen(scale: f32) -> f64 {
    scale.to_string().parse().unwrap_or(f64::from(scale))
}

/// The parts of a `POST /cards` form as sent; unknown parts are skipped.
#[derive(Debug, Default)]
struct Form {
    gpx: Option<Bytes>,
    runner: Option<String>,
    event: Option<String>,
    city: Option<String>,
    date: Option<String>,
    scale: Option<String>,
    format: Option<String>,
    variant: Option<String>,
    theme: Option<String>,
}

impl Form {
    async fn read(mut multipart: Multipart) -> Result<Self, ApiError> {
        let mut form = Self::default();
        while let Some(field) = multipart.next_field().await? {
            let text = match field.name() {
                Some("gpx") => {
                    form.gpx = Some(field.bytes().await?);
                    continue;
                }
                Some("runner") => &mut form.runner,
                Some("event") => &mut form.event,
                Some("city") => &mut form.city,
                Some("date") => &mut form.date,
                Some("scale") => &mut form.scale,
                Some("format") => &mut form.format,
                Some("variant") => &mut form.variant,
                Some("theme") => &mut form.theme,
                _ => continue,
            };
            *text = Some(field.text().await?);
        }
        Ok(form)
    }

    /// The GPX bytes, the card request and the answer format, checked.
    /// Optional parts that are blank count as not given.
    fn into_request(self) -> Result<(Bytes, CardRequest, Format), ApiError> {
        let gpx = self
            .gpx
            .filter(|gpx| !gpx.is_empty())
            .ok_or_else(|| ApiError::bad_request("gpx is missing or empty"))?;
        let runner = given(self.runner)
            .ok_or_else(|| ApiError::bad_request("runner is missing or empty"))?;
        let date = given(self.date)
            .map(|date| parse_date(&date))
            .transpose()
            .map_err(|e| ApiError::bad_request(format!("invalid date: {e}")))?;
        let scale = given(self.scale)
            .map(|scale| parse_bounded_scale(&scale))
            .transpose()
            .map_err(|e| ApiError::bad_request(format!("invalid scale: {e}")))?
            .unwrap_or(DEFAULT_SCALE);
        let format = given(self.format)
            .map(|format| format.parse())
            .transpose()
            .map_err(|e| ApiError::bad_request(format!("invalid format: {e}")))?
            .unwrap_or(Format::Png);
        let variant = given(self.variant)
            .map(|variant| variant.parse())
            .transpose()
            .map_err(|e| ApiError::bad_request(format!("invalid variant: {e}")))?
            .unwrap_or_default();
        let theme = given(self.theme)
            .map(|theme| theme.parse())
            .transpose()
            .map_err(|e| ApiError::bad_request(format!("invalid theme: {e}")))?
            .unwrap_or_default();
        let request = CardRequest {
            runner,
            event: given(self.event),
            city: given(self.city),
            date,
            scale,
            variant,
            theme,
        };
        Ok((gpx, request, format))
    }
}

/// `text` trimmed, or `None` when nothing is left.
fn given(text: Option<String>) -> Option<String> {
    text.map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

/// A raster scale above 0 and at most [`MAX_SCALE`].
fn parse_bounded_scale(s: &str) -> Result<f32, String> {
    let scale = parse_scale(s)?;
    if scale <= MAX_SCALE {
        Ok(scale)
    } else {
        Err(format!("must be at most {MAX_SCALE}, got {scale}"))
    }
}

/// The image format of the answer.
#[derive(Debug, Clone, Copy)]
enum Format {
    Png,
    Svg,
}

impl Format {
    fn name(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Svg => "svg",
        }
    }

    /// The card in this format, with its splits and the pipeline's time.
    fn render(
        self,
        pipeline: &Pipeline,
        gpx: &[u8],
        request: &CardRequest,
    ) -> Result<RenderedCard, Error> {
        Ok(match self {
            Self::Png => {
                let rendered = pipeline.render(gpx, request)?;
                RenderedCard {
                    card: Card::Png(rendered.png),
                    splits: rendered.splits,
                    time: total(&rendered.timings),
                }
            }
            Self::Svg => {
                let rendered = pipeline.render_svg(gpx, request)?;
                RenderedCard {
                    card: Card::Svg(rendered.svg),
                    splits: rendered.splits,
                    time: total(&rendered.timings),
                }
            }
        })
    }
}

/// The sum of the stage times.
fn total(timings: &[(&'static str, Duration)]) -> Duration {
    timings.iter().map(|(_, time)| *time).sum()
}

impl FromStr for Format {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "png" => Ok(Self::Png),
            "svg" => Ok(Self::Svg),
            other => Err(format!("expected png or svg, got {other:?}")),
        }
    }
}

/// A rendered card, answered inline under a fixed file name.
enum Card {
    Png(Vec<u8>),
    Svg(String),
}

impl Card {
    /// The size of the answer body in bytes.
    fn len(&self) -> usize {
        match self {
            Card::Png(png) => png.len(),
            Card::Svg(svg) => svg.len(),
        }
    }
}

/// A card and what the pipeline measured while rendering it.
struct RenderedCard {
    card: Card,
    splits: usize,
    /// The time of all pipeline stages together.
    time: Duration,
}

impl IntoResponse for Card {
    fn into_response(self) -> Response {
        match self {
            Card::Png(png) => (
                [
                    (header::CONTENT_TYPE, "image/png"),
                    (header::CONTENT_DISPOSITION, "inline; filename=\"card.png\""),
                ],
                png,
            )
                .into_response(),
            Card::Svg(svg) => (
                [
                    (header::CONTENT_TYPE, "image/svg+xml; charset=utf-8"),
                    (header::CONTENT_DISPOSITION, "inline; filename=\"card.svg\""),
                ],
                svg,
            )
                .into_response(),
        }
    }
}

/// An error answer: the status and a `{"error": "<message>"}` body.
#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    /// A 500 whose details go to the log only.
    fn internal() -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "rendering failed")
    }

    /// The answer to a pipeline `error`. Faults in the uploaded track are the
    /// client's (400 with the pipeline's message); everything after the layout
    /// is the server's (500, the details logged at ERROR with `request_id`).
    fn from_render(error: Error, request_id: &str) -> Self {
        match error {
            Error::Gpx(_)
            | Error::TooFewPoints
            | Error::NoTimestamps
            | Error::MissingTimestamp { .. }
            | Error::TimeNotMonotonic { .. }
            | Error::TooShort => Self::bad_request(error.to_string()),
            Error::Template(_) | Error::Svg(_) | Error::Png(_) | Error::Io(_) => {
                tracing::error!(request_id, %error, "rendering failed");
                Self::internal()
            }
        }
    }

    /// The answer to a card task that panicked or was cancelled: a 500, the
    /// details logged at ERROR with `request_id`.
    fn from_task(error: &JoinError, request_id: &str) -> Self {
        tracing::error!(request_id, %error, "the card task did not finish");
        Self::internal()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

impl From<MultipartRejection> for ApiError {
    fn from(rejection: MultipartRejection) -> Self {
        Self::new(rejection.status(), rejection.body_text())
    }
}

/// A body over the limit is a 413, a broken body a 400.
impl From<MultipartError> for ApiError {
    fn from(error: MultipartError) -> Self {
        Self::new(error.status(), error.body_text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use axum::http::StatusCode;
    use std::io;
    use std::sync::Mutex;

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

    /// What `f` returns and the JSON lines it logged.
    fn logged<T>(f: impl FnOnce() -> T) -> (T, Vec<Value>) {
        let captured = Captured::default();
        let writer = captured.clone();
        let subscriber = tracing_subscriber::fmt()
            .json()
            .flatten_event(true)
            .with_current_span(false)
            .with_span_list(false)
            .with_writer(move || writer.clone())
            .finish();
        let result = tracing::subscriber::with_default(subscriber, f);
        let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
        let lines = log
            .lines()
            .map(|line| serde_json::from_str(line).expect("a json log line"))
            .collect();
        (result, lines)
    }

    #[test]
    fn render_errors_are_internal_and_logged_with_the_request_id() {
        for (error, details) in [
            (
                Error::Svg("cannot rasterise at 0x0".into()),
                "cannot rasterise at 0x0",
            ),
            (Error::Io(io::Error::other("disk full")), "disk full"),
        ] {
            let (api, lines) = logged(|| ApiError::from_render(error, "abc"));
            assert_eq!(api.status, StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(api.message, "rendering failed");
            assert_eq!(lines.len(), 1, "{lines:?}");
            let line = &lines[0];
            assert_eq!(line["level"], "ERROR");
            assert_eq!(line["message"], "rendering failed");
            assert_eq!(line["request_id"], "abc");
            assert!(line["error"].as_str().unwrap().contains(details), "{line}");
        }
    }

    #[test]
    fn track_errors_are_bad_requests_with_the_pipeline_message_and_not_logged() {
        let (api, lines) =
            logged(|| ApiError::from_render(Error::TimeNotMonotonic { index: 4 }, "abc"));
        assert_eq!(api.status, StatusCode::BAD_REQUEST);
        assert_eq!(
            api.message,
            "timestamp at point 4 is earlier than the previous one"
        );
        assert!(lines.is_empty(), "{lines:?}");
    }

    #[tokio::test]
    async fn a_card_task_that_did_not_finish_is_internal_and_logged_with_the_request_id() {
        let task = tokio::spawn(std::future::pending::<()>());
        task.abort();
        let error = task.await.unwrap_err();
        let (api, lines) = logged(|| ApiError::from_task(&error, "abc"));
        assert_eq!(api.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(api.message, "rendering failed");
        assert_eq!(lines.len(), 1, "{lines:?}");
        let line = &lines[0];
        assert_eq!(line["level"], "ERROR");
        assert_eq!(line["message"], "the card task did not finish");
        assert_eq!(line["request_id"], "abc");
        assert!(
            line["error"].as_str().unwrap().contains("cancelled"),
            "{line}"
        );
    }

    #[test]
    fn latencies_are_milliseconds_rounded_to_a_tenth() {
        assert_eq!(millis(Duration::ZERO), 0.0);
        assert_eq!(millis(Duration::from_micros(1_234)), 1.2);
        assert_eq!(millis(Duration::from_micros(1_276)), 1.3);
        assert_eq!(millis(Duration::from_secs(2)), 2000.0);
    }

    #[test]
    fn a_scale_keeps_its_decimal_digits_as_f64() {
        assert_eq!(widen(0.3), 0.3);
        assert_eq!(widen(2.0), 2.0);
        assert_eq!(widen(1.25), 1.25);
    }
}
