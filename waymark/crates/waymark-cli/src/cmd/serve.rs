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

use super::open_project;

/// The default loopback port.
#[cfg(feature = "web")]
const DEFAULT_PORT: u16 = waymark_web::DEFAULT_PORT;
#[cfg(not(feature = "web"))]
const DEFAULT_PORT: u16 = 7331;

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
pub fn run(
    args: ServeArgs,
    project: Option<&Utf8Path>,
    json: bool,
) -> Result<u8, WaymarkError> {
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
fn serve(
    project: waymark_core::Project,
    args: ServeArgs,
    _json: bool,
) -> Result<u8, WaymarkError> {
    let _ = (project, args);
    Err(WaymarkError::internal(
        "the Web variant's serve implementation is not wired yet",
    ))
}
