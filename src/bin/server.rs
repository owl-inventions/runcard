//! `runcard-server`: the card pipeline over HTTP (`POST /cards`, `GET /variants`,
//! `GET /health`).

use std::io::IsTerminal;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::http::HeaderValue;
use clap::{Parser, ValueEnum};
use runcard::pipeline::Pipeline;
use runcard::server::{self, Config};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

const MIB: u64 = 1024 * 1024;

/// Serves runcard over HTTP: POST a GPX file to /cards and get the card back
/// as PNG or SVG.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Address to listen on.
    #[arg(
        long,
        value_name = "ADDR",
        env = "RUNCARD_BIND",
        default_value = "0.0.0.0:8080"
    )]
    bind: String,
    /// Origin a browser may call the API from, `*` for any; repeatable [default: none].
    #[arg(
        long = "allow-origin",
        value_name = "ORIGIN",
        env = "RUNCARD_ALLOW_ORIGIN",
        value_delimiter = ',',
        value_parser = parse_origin
    )]
    allow_origins: Vec<HeaderValue>,
    /// Largest accepted request body, in MiB.
    #[arg(
        long,
        value_name = "N",
        env = "RUNCARD_MAX_BODY_MIB",
        default_value_t = 20,
        value_parser = clap::value_parser!(u32).range(1..)
    )]
    max_body_mib: u32,
    /// How log lines are written to stderr.
    #[arg(
        long,
        value_name = "FORMAT",
        env = "RUNCARD_LOG_FORMAT",
        value_enum,
        default_value_t = LogFormat::Auto
    )]
    log_format: LogFormat,
}

/// The shape of the log lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum LogFormat {
    /// JSON when stderr is not a terminal (containers), text when it is.
    Auto,
    /// One JSON object per line.
    Json,
    /// Plain lines, coloured on a terminal.
    Text,
}

impl LogFormat {
    /// Whether the lines are JSON, given whether stderr is a terminal.
    fn is_json(self, stderr_is_terminal: bool) -> bool {
        match self {
            Self::Auto => !stderr_is_terminal,
            Self::Json => true,
            Self::Text => false,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    init_logging(args.log_format);

    let config = Config {
        max_body: usize::try_from(u64::from(args.max_body_mib) * MIB)
            .context("--max-body-mib is too large for this platform")?,
        allow_origins: args.allow_origins,
    };
    let pipeline = Pipeline::new().context("loading the card template and fonts")?;
    let app = server::router(Arc::new(pipeline), &config);

    let listener = TcpListener::bind(&args.bind)
        .await
        .with_context(|| format!("binding {}", args.bind))?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("serving")?;
    Ok(())
}

/// Logs to stderr in `format`, filtered by `RUST_LOG` (default `info`). JSON
/// lines carry `timestamp`, `level`, `target`, `message` and the event's
/// fields at the top level, without spans.
fn init_logging(format: LogFormat) {
    let terminal = std::io::stderr().is_terminal();
    let logs = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr);
    if format.is_json(terminal) {
        logs.json()
            .flatten_event(true)
            .with_current_span(false)
            .with_span_list(false)
            .with_ansi(false)
            .init();
    } else {
        logs.with_ansi(terminal).init();
    }
}

/// Resolves on ctrl-c or, on Unix, SIGTERM (what Docker and systemd send).
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "cannot listen for ctrl-c");
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                term.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutting down");
}

/// Parses an `--allow-origin` value into a header value.
fn parse_origin(s: &str) -> Result<HeaderValue, String> {
    HeaderValue::from_str(s.trim()).map_err(|e| format!("not a valid origin: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_logs_text_to_a_terminal_and_json_anywhere_else() {
        assert!(!LogFormat::Auto.is_json(true));
        assert!(LogFormat::Auto.is_json(false));
    }

    #[test]
    fn json_and_text_hold_whatever_stderr_is() {
        for terminal in [true, false] {
            assert!(LogFormat::Json.is_json(terminal));
            assert!(!LogFormat::Text.is_json(terminal));
        }
    }

    #[test]
    fn the_log_format_is_auto_unless_named() {
        let parse = |args: &[&str]| {
            Args::try_parse_from([&["runcard-server"], args].concat()).map(|args| args.log_format)
        };
        assert_eq!(parse(&[]).unwrap(), LogFormat::Auto);
        assert_eq!(parse(&["--log-format", "json"]).unwrap(), LogFormat::Json);
        assert_eq!(parse(&["--log-format", "text"]).unwrap(), LogFormat::Text);
        assert_eq!(parse(&["--log-format", "auto"]).unwrap(), LogFormat::Auto);
        assert!(parse(&["--log-format", "yaml"]).is_err());
    }
}
