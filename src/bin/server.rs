//! `runcard-server`: the card pipeline over HTTP (`POST /cards`, `GET /health`).

use std::io::IsTerminal;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::http::HeaderValue;
use clap::Parser;
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();

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
