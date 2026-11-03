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

    let start = match args.project {
        Some(directory) => directory,
        None => match std::env::current_dir()
            .ok()
            .and_then(|directory| Utf8PathBuf::from_path_buf(directory).ok())
        {
            Some(directory) => directory,
            None => {
                eprintln!("error: the working directory is not UTF-8");
                return ExitCode::from(1);
            }
        },
    };

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
