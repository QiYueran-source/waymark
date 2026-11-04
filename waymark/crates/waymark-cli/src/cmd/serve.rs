//! `waymark serve`: the read-only Web view over loopback.
//!
//! The command exists in every build. A basic build discovers the project the
//! same way every other command does and then refuses with the registered
//! `WEB_NOT_INCLUDED`: the fix is to install the Web package, and this command
//! never downloads, installs, or shells out to anything.
//!
//! A Web build binds the embedded frontend on `127.0.0.1` only. It reports
//! readiness before it blocks — `serving http://127.0.0.1:<port>` on stderr in
//! human mode, or the `{"url": …}` envelope on stdout with `--json` — and never
//! falls back to another port when the requested one is busy.

use camino::Utf8Path;
use clap::Args;
use waymark_core::error::WaymarkError;
use waymark_core::DEFAULT_PORT;

use super::open_project;

#[cfg(feature = "web")]
use super::{emit, err_line, EXIT_OK};
#[cfg(feature = "web")]
use serde_json::json;
#[cfg(feature = "web")]
use waymark_core::envelope::{success_envelope, Meta};

/// Arguments of `waymark serve`.
#[derive(Debug, Args)]
pub struct ServeArgs {
    /// The loopback port to bind; 0 asks the operating system for a free port.
    #[arg(long, value_name = "PORT", default_value_t = DEFAULT_PORT)]
    pub port: u16,
    /// Open the Web view in the default browser once the server is ready.
    #[arg(long)]
    pub open: bool,
}

/// Run `waymark serve`.
///
/// Discovery comes first in both builds: a missing or invalid project fails
/// before a listener would bind and before anything is created or modified.
pub fn run(args: ServeArgs, project: Option<&Utf8Path>, json: bool) -> Result<u8, WaymarkError> {
    let project = open_project(project)?;
    serve(project, args, json)
}

#[cfg(not(feature = "web"))]
fn serve(
    _project: waymark_core::Project,
    _args: ServeArgs,
    _json: bool,
) -> Result<u8, WaymarkError> {
    Err(WaymarkError::web_not_included())
}

#[cfg(feature = "web")]
fn serve(project: waymark_core::Project, args: ServeArgs, json: bool) -> Result<u8, WaymarkError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|source| {
            WaymarkError::internal(format!("cannot start the Web runtime: {source}"))
        })?;
    runtime.block_on(serve_embedded(project, args, json))
}

/// Bind, report readiness, then serve until Ctrl-C.
///
/// Readiness is reported only after the listener exists, so the printed port is
/// the one a client can use; `--port 0` therefore reports the port the operating
/// system chose.
#[cfg(feature = "web")]
async fn serve_embedded(
    project: waymark_core::Project,
    args: ServeArgs,
    json: bool,
) -> Result<u8, WaymarkError> {
    let config = waymark_web::WebConfig::embedded().with_port(args.port);
    let running = waymark_web::bind(project, config)
        .await
        .map_err(map_web_error)?;
    let url = format!("http://127.0.0.1:{}", running.addr().port());

    if json {
        emit(&success_envelope(
            json!({"url": url}),
            Meta::direct_file_scoped(),
        ))?;
    } else {
        err_line(&format!("serving {url}"));
    }

    if args.open {
        if let Err(source) = open_in_browser(&url) {
            // Never a serve failure: the view is already available and the URL
            // was printed, so a browser that is not installed is a note.
            err_line(&format!(
                "warning: could not launch the browser opener: {source}"
            ));
        }
    }

    tokio::signal::ctrl_c()
        .await
        .map_err(|source| WaymarkError::internal(format!("cannot wait for Ctrl-C: {source}")))?;
    running.shutdown().await.map_err(map_web_error)?;
    Ok(EXIT_OK)
}

/// The platform's browser launcher.
#[cfg(feature = "web")]
fn opener() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

/// Launch the browser without waiting for it.
///
/// Only a failure to launch is reported; the browser process is not ours to
/// supervise, and a launcher that exits later must not take the server with it.
#[cfg(feature = "web")]
fn open_in_browser(url: &str) -> std::io::Result<()> {
    let opener = opener();
    std::process::Command::new(opener)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|source| std::io::Error::new(source.kind(), format!("{opener}: {source}")))
}

/// Translate a Web-service failure into the registered taxonomy.
///
/// A port already held is the one expected failure and becomes `PORT_IN_USE`;
/// every other startup fault is an environment or internal fault, and there is
/// never a fallback to a different port.
#[cfg(feature = "web")]
fn map_web_error(error: waymark_web::WebError) -> WaymarkError {
    use std::io::ErrorKind;

    match error {
        waymark_web::WebError::Bind { port, source } if source.kind() == ErrorKind::AddrInUse => {
            WaymarkError::port_in_use(port)
        }
        waymark_web::WebError::Bind { port, source } => {
            WaymarkError::io(format!("127.0.0.1:{port}"), "bind", &source)
        }
        waymark_web::WebError::Address(source) => {
            WaymarkError::io("the loopback listener", "read the address of", &source)
        }
        waymark_web::WebError::Serve(source) => {
            WaymarkError::internal(format!("the Web service stopped: {source}"))
        }
        waymark_web::WebError::Signal(source) => {
            WaymarkError::internal(format!("cannot wait for Ctrl-C: {source}"))
        }
        waymark_web::WebError::MissingAssets(message) => WaymarkError::internal(message),
    }
}
