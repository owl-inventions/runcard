//! The HTTP API: `POST /cards` renders an uploaded GPX track as a card,
//! `GET /variants` lists the card looks, `GET /health` answers that the
//! server is up.

use std::str::FromStr;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::multipart::{Multipart, MultipartError, MultipartRejection};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use serde_json::{json, Value};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::error::Error;
use crate::pipeline::{parse_date, parse_scale, CardRequest, Pipeline};
use crate::variant::Variant;

/// The request body limit unless configured otherwise: 20 MiB.
pub const DEFAULT_MAX_BODY: usize = 20 * 1024 * 1024;
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
        .route("/health", get(health))
        .route("/variants", get(variants))
        .route("/cards", post(create_card))
        .with_state(pipeline)
        .layer(DefaultBodyLimit::max(config.max_body));
    let router = match cors(&config.allow_origins) {
        Some(cors) => router.layer(cors),
        None => router,
    };
    router.layer(TraceLayer::new_for_http())
}

/// CORS for the listed origins, any origin if one of them is `*`, none without origins.
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
                .allow_origin(origins.to_vec()),
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
    multipart: Result<Option<Multipart>, MultipartRejection>,
) -> Result<Card, ApiError> {
    let multipart = multipart?.ok_or_else(|| {
        ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "expected a multipart/form-data body",
        )
    })?;
    let (gpx, request, format) = Form::read(multipart).await?.into_request()?;
    tokio::task::spawn_blocking(move || match format {
        Format::Png => pipeline.png(&gpx[..], &request).map(Card::Png),
        Format::Svg => pipeline.svg(&gpx[..], &request).map(Card::Svg),
    })
    .await
    .map_err(|error| {
        tracing::error!(%error, "the card task did not finish");
        ApiError::internal()
    })?
    .map_err(ApiError::from)
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
        let request = CardRequest {
            runner,
            event: given(self.event),
            city: given(self.city),
            date,
            scale,
            variant,
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
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

/// Faults in the uploaded track are the client's; everything after the
/// layout is the server's.
impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        match error {
            Error::Gpx(_)
            | Error::TooFewPoints
            | Error::NoTimestamps
            | Error::MissingTimestamp { .. }
            | Error::TimeNotMonotonic { .. }
            | Error::TooShort => Self::bad_request(error.to_string()),
            Error::Template(_) | Error::Svg(_) | Error::Png(_) | Error::Io(_) => {
                tracing::error!(%error, "rendering failed");
                Self::internal()
            }
        }
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

    #[test]
    fn render_errors_are_internal_and_keep_their_details_to_the_log() {
        for error in [
            Error::Svg("cannot rasterise at 0x0".into()),
            Error::Io(std::io::Error::other("disk full")),
        ] {
            let api = ApiError::from(error);
            assert_eq!(api.status, StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(api.message, "rendering failed");
        }
    }

    #[test]
    fn track_errors_are_bad_requests_with_the_pipeline_message() {
        let api = ApiError::from(Error::TimeNotMonotonic { index: 4 });
        assert_eq!(api.status, StatusCode::BAD_REQUEST);
        assert_eq!(
            api.message,
            "timestamp at point 4 is earlier than the previous one"
        );
    }
}
