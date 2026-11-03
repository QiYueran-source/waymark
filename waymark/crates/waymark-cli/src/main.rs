//! The `waymark` command-line adapter.
//!
//! The adapter owns three things and nothing else:
//!
//! 1. **Parsing arguments** into a domain request (`clap`, derive mode).
//! 2. **Deciding where output goes**: in JSON mode the versioned envelope goes to
//!    stdout and nothing else does; human narrative goes to stderr, so
//!    `waymark show ID > out.md` writes the document and not a status line.
//! 3. **Mapping a [`WaymarkError`] to a process exit code** from the registry —
//!    one function, no per-command table.
//!
//! Every rule lives in `waymark-core`. If a rule appears here, it is in the wrong
//! crate.

use std::process::ExitCode;
use std::sync::Arc;

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use waymark_core::ops::OpOptions;
use waymark_core::time::SystemClock;

mod cmd;

use cmd::create::CreateArgs;
use cmd::init::InitArgs;
use cmd::show::ShowArgs;
use cmd::{EXIT_OK, USAGE_EXIT};

#[derive(Debug, Parser)]
#[command(
    name = "waymark",
    about = "Track work as Markdown documents in the repository",
    version,
    disable_help_subcommand = true
)]
struct Cli {
    /// Emit the versioned JSON envelope on stdout.
    #[arg(long, global = true)]
    json: bool,

    /// Operate on the project at this directory instead of discovering one.
    #[arg(long, global = true, value_name = "DIR")]
    project: Option<Utf8PathBuf>,

    /// Wait at most this many milliseconds for the project write lock.
    #[arg(long, global = true, value_name = "MS")]
    lock_timeout_ms: Option<u64>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create the project configuration and metadata directory.
    Init(InitArgs),
    /// Create one work item.
    Create(CreateArgs),
    /// Read one work item by its stable ID.
    Show(ShowArgs),
}

fn main() -> ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let cli = match Cli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(error) => {
            // `clap` renders help and version on stdout and reports them as
            // "errors"; those are successful invocations. Anything else is a
            // usage error: exit 1 with `USAGE_ERROR`, which lives here and not
            // in the registry.
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                let _ = error.print();
                return ExitCode::from(EXIT_OK);
            }
            cmd::report_usage_error(&error, cmd::json_requested(&args));
            return ExitCode::from(USAGE_EXIT);
        }
    };

    let json = cli.json;
    match run(cli) {
        Ok(()) => ExitCode::from(EXIT_OK),
        Err(error) => {
            // One exit mapping, from the registry. Warnings and findings never
            // reach here: they are attached to `_meta` and change nothing.
            let code = cmd::exit_code(&error);
            cmd::report_failure(&error, json);
            ExitCode::from(code)
        }
    }
}

fn run(cli: Cli) -> Result<(), waymark_core::WaymarkError> {
    let json = cli.json;
    let options = OpOptions::new()
        .with_clock(Arc::new(SystemClock))
        .with_lock_wait(cmd::lock_wait(cli.lock_timeout_ms));

    match cli.command {
        Command::Init(args) => cmd::init::run(args, cli.project.as_deref(), json),
        Command::Create(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::create::run(&project, args, &options, json)
        }
        Command::Show(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::show::run(&project, &args, json)
        }
    }
}
