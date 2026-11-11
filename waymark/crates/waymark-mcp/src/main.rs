//! The `waymark-mcp` binary: the Waymark MCP server over stdio.
//!
//! Transport and process boundary only. Every tool, every envelope, and every
//! error classification lives in the library, so the binary cannot drift from
//! the adapter the tests exercise.

use std::process::ExitCode;

use camino::Utf8PathBuf;
use clap::Parser;
use rmcp::{transport::stdio, ServiceExt};
use waymark_core::error::WaymarkError;
use waymark_mcp::WaymarkServer;

/// Waymark MCP server over stdio.
#[derive(Debug, Parser)]
#[command(
    name = "waymark-mcp",
    about = "Serve the Waymark work-item tracker over the Model Context Protocol",
    version,
    disable_help_subcommand = true
)]
struct Args {
    /// Operate on the project at this directory instead of discovering one from
    /// the working directory.
    #[arg(long, value_name = "DIR")]
    project: Option<Utf8PathBuf>,

    /// Check the install instead of serving: print OK and exit 0 when a Waymark
    /// project is found, otherwise print the reason on stderr and exit nonzero.
    ///
    /// A CLI connector asks its tool "are you ready" and expects this shape.
    #[arg(long, conflicts_with = "forget")]
    ready: bool,

    /// Clear any per-user state this server keeps. The server keeps none, so
    /// this succeeds without doing anything and keeps a logout step idempotent.
    #[arg(long, conflicts_with = "ready")]
    forget: bool,
}

fn main() -> ExitCode {
    let args = match Args::try_parse() {
        Ok(args) => args,
        Err(error) => {
            // Help and version are successful invocations; clap prints them and
            // reports them as errors.
            let _ = error.print();
            let usage = !matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            );
            return ExitCode::from(if usage { 1 } else { 0 });
        }
    };

    // Both checks answer on a single line and never start the transport, so a
    // caller can run them without a client attached.
    if args.forget {
        // The server keeps no per-user state: project settings live in the
        // project's own `.waymark` directory and credentials do not exist.
        println!("OK");
        return ExitCode::SUCCESS;
    }

    let start = match args.project {
        Some(directory) => directory,
        None => std::env::current_dir()
            .ok()
            .and_then(|directory| Utf8PathBuf::from_path_buf(directory).ok())
            .unwrap_or_else(|| {
                // Reported per command below, so `--ready` and serving share one
                // failure path.
                Utf8PathBuf::new()
            }),
    };

    if args.ready {
        return ready(&start);
    }

    if start.as_str().is_empty() {
        eprintln!("error: the working directory is not UTF-8");
        return ExitCode::from(1);
    }

    if !start.is_dir() {
        let error = WaymarkError::project_not_found(start.to_string());
        eprintln!("error: {}", error.message());
        return ExitCode::from(exit_code(&error));
    }

    // A missing or unusable project is not fatal: `waymark_project_init` must
    // work where no project exists, and `waymark_config_validate` must report a
    // broken configuration instead of refusing to start. The narrative goes to
    // stderr because stdout is the protocol channel.
    match WaymarkServer::new(start.clone()).project() {
        Ok(project) => eprintln!("waymark-mcp: project {}", project.root()),
        Err(error) => eprintln!(
            "waymark-mcp: no usable project at or above {start}: {}; \
             waymark_project_init can create one, other tools will report it",
            error.message()
        ),
    }

    run(start)
}

/// Answer `--ready` without starting the transport.
///
/// Ready means "this install can serve a project from here". A caller that runs
/// it while no project exists yet gets a nonzero exit and the reason, which is
/// the useful answer: the install is fine, the project is not there.
fn ready(start: &Utf8PathBuf) -> ExitCode {
    if start.as_str().is_empty() {
        eprintln!("the working directory is not UTF-8");
        return ExitCode::from(1);
    }
    match WaymarkServer::new(start.clone()).project() {
        Ok(project) => {
            println!("OK");
            eprintln!("waymark-mcp: project {}", project.root());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!(
                "no Waymark project at or above {start}: {}",
                error.message()
            );
            ExitCode::from(exit_code(&error))
        }
    }
}

/// Serve until the client closes the transport.
#[tokio::main]
async fn run(start: Utf8PathBuf) -> ExitCode {
    let server = WaymarkServer::new(start);
    let service = match server.serve(stdio()).await {
        Ok(service) => service,
        Err(error) => {
            eprintln!("error: cannot start the MCP stdio transport: {error}");
            return ExitCode::from(EXIT_INTERNAL);
        }
    };
    match service.waiting().await {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: the MCP transport failed: {error}");
            ExitCode::from(EXIT_INTERNAL)
        }
    }
}

/// The process exit code the registry assigns to a startup failure.
fn exit_code(error: &WaymarkError) -> u8 {
    error
        .exit_class()
        .and_then(|class| u8::try_from(class.as_i32()).ok())
        .unwrap_or(EXIT_INTERNAL)
}

/// Exit class 7, used when a failure carries no exit class at all.
const EXIT_INTERNAL: u8 = 7;
