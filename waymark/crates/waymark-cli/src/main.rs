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
use cmd::query::{QueryArgs, SearchArgs, StatsArgs};

use cmd::config::ConfigArgs;
use cmd::create::CreateArgs;
use cmd::edit::{BodyArgs, FieldArgs, TitleArgs};
use cmd::init::InitArgs;
use cmd::link::LinkArgs;
use cmd::list::ListArgs;
use cmd::parent::ParentArgs;
use cmd::project::ProjectArgs;
use cmd::relationship::DependencyArgs;
use cmd::show::ShowArgs;
use cmd::status::{StatusArgs, TransitionArgs};
use cmd::tree::{DepArgs, TreeArgs};
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
    /// Change the lifecycle state of one item.
    Status(StatusArgs),
    /// Replace the title of one item.
    Title(TitleArgs),
    /// Replace or clear the body of one item.
    Body(BodyArgs),
    /// Set and clear custom fields of one item.
    Field(FieldArgs),
    /// Move one item to `in_progress`.
    Start(TransitionArgs),
    /// Move one item to `closed`.
    Close(TransitionArgs),
    /// Move one item back to `open`.
    Reopen(TransitionArgs),
    /// Add or remove a hard dependency edge.
    Dependency(DependencyArgs),
    /// Set or clear the single parent of an item.
    Parent(ParentArgs),
    /// Add or remove a symmetric soft link.
    Link(LinkArgs),
    /// Read the hierarchy tree, bounded by depth.
    Tree(TreeArgs),
    /// Read dependency structures.
    Dep(DepArgs),
    /// List active work items.
    List(ListArgs),
    /// List items whose dependencies are all closed.
    Ready(ListArgs),
    /// List items waiting on an unfinished or missing dependency.
    Blocked(ListArgs),
    /// Run a list read driven by a structured JSON filter document.
    Query(QueryArgs),
    /// Search titles, frontmatter, and bodies case-insensitively.
    Search(SearchArgs),
    /// Report counts only: totals, by status, by category, ready, and blocked.
    Stats(StatsArgs),
    /// Inspect the project itself.
    Project(ProjectArgs),
    /// Read, validate, and change the project configuration.
    Config(ConfigArgs),
    /// Validate the whole project.
    Validate,
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
        Command::Status(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::status::run(&project, args, &options, json)
        }
        Command::Title(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::edit::run_title(&project, args, &options, json)
        }
        Command::Body(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::edit::run_body(&project, args, &options, json)
        }
        Command::Field(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::edit::run_field(&project, args, &options, json)
        }
        Command::Start(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::status::convenience(
                &project,
                &args,
                waymark_core::Status::InProgress,
                &options,
                json,
            )
        }
        Command::Close(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::status::convenience(
                &project,
                &args,
                waymark_core::Status::Closed,
                &options,
                json,
            )
        }
        Command::Reopen(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::status::convenience(&project, &args, waymark_core::Status::Open, &options, json)
        }
        Command::Dependency(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::relationship::run(&project, args, &options, json)
        }
        Command::Parent(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::parent::run(&project, args, &options, json)
        }
        Command::Link(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::link::run(&project, args, &options, json)
        }
        Command::Tree(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::tree::run(&project, &args, json)
        }
        Command::Dep(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::tree::run_dep(&project, args, &options, json)
        }
        Command::List(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::list::run(&project, &args, cmd::list::Which::All, json)
        }
        Command::Ready(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::list::run(&project, &args, cmd::list::Which::Ready, json)
        }
        Command::Blocked(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::list::run(&project, &args, cmd::list::Which::Blocked, json)
        }
        Command::Query(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::query::run_query(&project, &args, json)
        }
        Command::Search(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::query::run_search(&project, &args, json)
        }
        Command::Stats(args) => {
            let project = cmd::open_project(cli.project.as_deref())?;
            cmd::query::run_stats(&project, &args, json)
        }
        Command::Project(args) => cmd::project::run(args, cli.project.as_deref(), json),
        Command::Config(args) => cmd::config::run(args, cli.project.as_deref(), &options, json),
        Command::Validate => cmd::project::run_validate(cli.project.as_deref(), json),
    }
}
