use std::io::IsTerminal;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use logmaker_core::{AppState, Config, router};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, fmt};

/// LogMaker: plugin-based log generation and delivery server.
#[derive(Debug, Parser)]
#[command(name = "logmaker", version)]
struct Cli {
    /// Address to listen on.
    #[arg(long, env = "LOGMAKER_BIND", default_value = "0.0.0.0")]
    bind: IpAddr,

    /// HTTP port.
    #[arg(long, env = "LOGMAKER_PORT", default_value_t = 19999, alias = "server.port")]
    port: u16,

    /// Directory for maker/sender/log/scenario definitions [default: ~/.logmaker-data].
    #[arg(long, env = "LOGMAKER_DATA_ROOT", alias = "data.root")]
    data_root: Option<PathBuf>,

    /// Directory for plugin libraries [default: ~/.logmaker-plugin].
    #[arg(long, env = "LOGMAKER_PLUGIN_ROOT", alias = "plugin.root")]
    plugin_root: Option<PathBuf>,

    /// Directory for daily-rotated log files (kept 30 days); empty disables file logging.
    #[arg(long, env = "LOGMAKER_LOG_DIR", default_value = "logs", value_parser = parse_path)]
    log_dir: PathBuf,
}

/// Like the default path parser, but allows an empty value.
fn parse_path(value: &str) -> Result<PathBuf, std::convert::Infallible> {
    Ok(PathBuf::from(value))
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
}

/// Logs to stdout and, when `log_dir` is set, to `log_dir/logmaker.<date>.log`.
/// The level is controlled by `RUST_LOG` (default `info`; the Kafka client's
/// per-attempt connection warnings are hidden, the sender reports failures itself).
fn init_logging(log_dir: &std::path::Path) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let filter = || EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,rskafka=error"));
    let stdout = fmt::layer()
        .with_ansi(std::io::stdout().is_terminal())
        .with_filter(filter());
    if log_dir.as_os_str().is_empty() {
        tracing_subscriber::registry().with(stdout).init();
        return None;
    }
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("logmaker")
        .filename_suffix("log")
        .max_log_files(30)
        .build(log_dir);
    match appender {
        Ok(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let file = fmt::layer().with_ansi(false).with_writer(writer).with_filter(filter());
            tracing_subscriber::registry().with(stdout).with(file).init();
            Some(guard)
        }
        Err(e) => {
            tracing_subscriber::registry().with(stdout).init();
            tracing::warn!("file logging disabled ({}): {e}", log_dir.display());
            None
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let _log_guard = init_logging(&cli.log_dir);

    let config = Config {
        data_root: cli.data_root.unwrap_or_else(|| home_dir().join(".logmaker-data")),
        plugin_root: cli.plugin_root.unwrap_or_else(|| home_dir().join(".logmaker-plugin")),
    };
    tracing::info!(
        "data root {}, plugin root {}",
        config.data_root.display(),
        config.plugin_root.display()
    );
    let state = tokio::task::spawn_blocking(move || AppState::start(&config)).await??;

    let address = SocketAddr::new(cli.bind, cli.port);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("cannot listen on {address}"))?;
    tracing::info!("LogMaker {} listening on http://{address}", env!("CARGO_PKG_VERSION"));
    axum::serve(listener, router(state.clone()))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("shutting down");
    tokio::task::spawn_blocking(move || state.shutdown()).await?;
    Ok(())
}
