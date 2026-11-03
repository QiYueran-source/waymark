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

use std::io::Write;
use std::process::ExitCode;
use std::sync::Arc;

use camino::Utf8PathBuf;
use clap::{Args, Parser, Subcommand};
use waymark_core::config::Config;
use waymark_core::envelope::{failure_envelope, success_envelope, Meta};
use waymark_core::error::{Code, WaymarkError};
use waymark_core::lock::LockWait;
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::{create, read, OpOptions};
use waymark_core::store::InitOptions;
use waymark_core::time::SystemClock;
use waymark_core::{Project, WorkItem};

/// The process exit code for a plain success.
const EXIT_OK: u8 = 0;

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

#[derive(Debug, Args)]
struct InitArgs {
    /// Category key to declare.
    #[arg(long, value_name = "KEY", default_value = waymark_core::config::DEFAULT_CATEGORY_KEY)]
    category: String,

    /// Repository-relative storage root.
    #[arg(long, value_name = "PATH", default_value = waymark_core::config::DEFAULT_STORAGE_ROOT)]
    storage_root: String,

    /// Advisory operator identity recorded in the configuration.
    #[arg(long, value_name = "NAME")]
    operator: Option<String>,

    /// IANA time zone used for human display.
    #[arg(long, value_name = "ZONE")]
    timezone: Option<String>,

    /// Replace an existing configuration instead of reusing it.
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Args)]
struct CreateArgs {
    /// Short single-line label.
    #[arg(long, value_name = "TEXT")]
    title: String,

    /// Category key the item belongs to; must already be declared.
    #[arg(long, value_name = "KEY")]
    category: String,

    /// Accountable identity.
    #[arg(long, value_name = "NAME")]
    owner: String,

    /// Markdown body, or `-` to read it from stdin.
    #[arg(long, value_name = "TEXT")]
    body: Option<String>,

    /// Validate and report what would be written, without writing.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct ShowArgs {
    /// Stable ID, for example `wm-3f7k9q2m4x8c`.
    #[arg(value_name = "ID")]
    id: String,

    /// Print the document's Markdown on stdout instead of a field summary.
    #[arg(long)]
    raw: bool,
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let kind = error.kind();
            let _ = error.print();
            // `clap` renders help and version on stdout and reports them as
            // "errors"; those are successful invocations. A real parse failure
            // is a usage error and exits 1.
            let code = if matches!(
                kind,
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                EXIT_OK
            } else {
                Code::UsageError.exit_class()
            };
            return ExitCode::from(code);
        }
    };

    let json = cli.json;
    match run(cli) {
        Ok(()) => ExitCode::from(EXIT_OK),
        Err(error) => {
            report_failure(&error, json);
            ExitCode::from(error.exit_class())
        }
    }
}

fn run(cli: Cli) -> Result<(), WaymarkError> {
    let json = cli.json;
    let options = OpOptions::new()
        .with_clock(Arc::new(SystemClock))
        .with_lock_wait(lock_wait(cli.lock_timeout_ms));

    match cli.command {
        Command::Init(args) => run_init(args, cli.project.as_deref(), json),
        Command::Create(args) => {
            let project = open_project(cli.project.as_deref())?;
            run_create(&project, args, &options, json)
        }
        Command::Show(args) => {
            let project = open_project(cli.project.as_deref())?;
            run_show(&project, &args, json)
        }
    }
}

fn lock_wait(milliseconds: Option<u64>) -> LockWait {
    match milliseconds {
        Some(0) => LockWait::immediate(),
        Some(value) => LockWait::new(std::time::Duration::from_millis(value)),
        None => LockWait::default_policy(),
    }
}

/// The directory the command was invoked from.
fn invocation_dir() -> Result<Utf8PathBuf, WaymarkError> {
    let cwd = std::env::current_dir()
        .map_err(|source| WaymarkError::io("the working directory", "read", &source))?;
    Utf8PathBuf::from_path_buf(cwd)
        .map_err(|_| WaymarkError::internal("the working directory is not UTF-8"))
}

/// Open the project named by `--project`, or discover it from the invocation
/// directory.
fn open_project(explicit: Option<&camino::Utf8Path>) -> Result<Project, WaymarkError> {
    match explicit {
        Some(dir) => Project::discover(dir),
        None => Project::discover(&invocation_dir()?),
    }
}

fn run_init(
    args: InitArgs,
    explicit: Option<&camino::Utf8Path>,
    json: bool,
) -> Result<(), WaymarkError> {
    let root = match explicit {
        Some(dir) => dir.to_path_buf(),
        None => invocation_dir()?,
    };

    let key = waymark_core::CategoryKey::parse(&args.category).map_err(|error| {
        WaymarkError::config_invalid("init", vec![format!("category: {error}")])
    })?;

    let mut config = Config::fresh();
    config.storage_root = args.storage_root.clone();
    config.categories.clear();
    config.categories.insert(
        key.clone(),
        waymark_core::config::Category {
            path: format!("{}/{}", args.storage_root, key.as_str()),
            name: waymark_core::config::default_category_name(&key),
        },
    );
    if let Some(operator) = &args.operator {
        config.operator = Some(operator.clone());
    }
    if let Some(timezone) = &args.timezone {
        config.timezone = waymark_core::time::parse_timezone(timezone)
            .map_err(|error| WaymarkError::config_invalid("init", vec![error.to_string()]))?;
    }

    let project = Project::init(
        &root,
        InitOptions {
            force: args.force,
            config: Some(config),
            operator: None,
        },
    )?;

    if json {
        let categories: Vec<serde_json::Value> = project
            .config()
            .categories
            .iter()
            .map(|(key, category)| {
                serde_json::json!({
                    "key": key.as_str(),
                    "path": category.path,
                    "name": category.name,
                })
            })
            .collect();
        emit(&success_envelope(
            serde_json::json!({
                "root": project.root().to_string(),
                "metadata_dir": project.metadata_dir().to_string(),
                "config_path": project.config_path().to_string(),
                "storage_root": project.config().storage_root,
                "categories": categories,
            }),
            Meta::direct_file_scoped(),
        ))?;
    } else {
        err_line(&format!(
            "initialised Waymark project at {}",
            project.root()
        ));
        err_line(&format!("  configuration  {}", project.config_path()));
        for (key, category) in &project.config().categories {
            err_line(&format!("  category {}  {}", key, category.path));
        }
    }
    Ok(())
}

fn run_create(
    project: &Project,
    args: CreateArgs,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    let body = match args.body.as_deref() {
        Some("-") => {
            let mut buffer = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut buffer)
                .map_err(|source| WaymarkError::io("stdin", "read", &source))?;
            Some(buffer)
        }
        Some(text) => Some(text.to_owned()),
        None => None,
    };

    let mut request = CreateRequest::new(args.title, args.category, args.owner);
    if let Some(body) = body {
        request = request.with_body(body);
    }
    let options = if args.dry_run {
        options.clone().dry_run()
    } else {
        options.clone()
    };

    let created = create::create(project, request, &options)?;

    if json {
        emit(&success_envelope(
            serde_json::json!({
                "id": created.item.id.as_str(),
                "path": created.path.to_string(),
                "written": created.written,
                "item": item_json(&created.item, &created.path),
            }),
            Meta::direct_file_scoped(),
        ))?;
    } else if created.written {
        err_line(&format!("created {}", created.item.id));
        err_line(&format!("  {}", created.path));
    } else {
        err_line(&format!("would create {}", created.item.id));
        err_line(&format!("  {}", created.path));
    }
    Ok(())
}

fn run_show(project: &Project, args: &ShowArgs, json: bool) -> Result<(), WaymarkError> {
    let read = read::read(project, &args.id)?;

    if json {
        emit(&success_envelope(
            serde_json::json!({
                "id": read.item.id.as_str(),
                "path": read.path.to_string(),
                "category": read.category.as_str(),
                "fingerprint": read.fingerprint.as_str(),
                "item": item_json(&read.item, &read.path),
                "relationships": {
                    "parent": read.item.parent.as_ref().map(|id| id.as_str().to_owned()),
                    "dependencies": read.item.dependencies.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
                    "soft_links": read.item.soft_links.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
                },
            }),
            read.meta,
        ))?;
    } else if args.raw {
        // The document itself on stdout, so it can be piped or redirected.
        print!("{}", waymark_core::serialize::to_document(&read.item));
    } else {
        err_line(&format!("{}  {}", read.item.id, read.item.title));
        err_line(&format!("  category    {}", read.item.category));
        err_line(&format!("  status      {}", read.item.status.as_str()));
        err_line(&format!("  owner       {}", read.item.owner));
        if let Some(creator) = &read.item.creator {
            err_line(&format!("  creator     {creator}"));
        }
        err_line(&format!("  created     {}", read.item.created_at));
        err_line(&format!("  updated     {}", read.item.updated_at));
        if let Some(closed_at) = read.item.closed_at {
            err_line(&format!("  closed      {closed_at}"));
        }
        if let Some(parent) = &read.item.parent {
            err_line(&format!("  parent      {parent}"));
        }
        if !read.item.dependencies.is_empty() {
            let ids: Vec<&str> = read
                .item
                .dependencies
                .iter()
                .map(|id| id.as_str())
                .collect();
            err_line(&format!("  depends on  {}", ids.join(", ")));
        }
        if !read.item.soft_links.is_empty() {
            let ids: Vec<&str> = read.item.soft_links.iter().map(|id| id.as_str()).collect();
            err_line(&format!("  soft links  {}", ids.join(", ")));
        }
        for (key, value) in &read.item.custom_fields {
            err_line(&format!("  {key:<11} {}", render_field_value(value)));
        }
        if let Some(archived_at) = read.item.archived_at {
            err_line(&format!("  archived    {archived_at}"));
        }
        err_line(&format!("  file        {}", read.path));
        if !read.item.body.is_empty() {
            err_line("");
            for line in read.item.body.lines() {
                err_line(line);
            }
        }
    }
    Ok(())
}

/// The machine form of one item: the persisted fields, no prose.
fn item_json(item: &WorkItem, path: &camino::Utf8Path) -> serde_json::Value {
    serde_json::json!({
        "schema": item.schema,
        "id": item.id.as_str(),
        "title": item.title,
        "category": item.category.as_str(),
        "status": item.status.as_str(),
        "owner": item.owner,
        "creator": item.creator,
        "created_at": item.created_at.to_string(),
        "updated_at": item.updated_at.to_string(),
        "closed_at": item.closed_at.map(|value| value.to_string()),
        "parent": item.parent.as_ref().map(|id| id.as_str().to_owned()),
        "dependencies": item.dependencies.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        "soft_links": item.soft_links.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        "custom_fields": item.custom_fields,
        "archived_at": item.archived_at.map(|value| value.to_string()),
        "archive_reason": item.archive_reason,
        "body": item.body,
        "path": path.to_string(),
    })
}

fn render_field_value(value: &waymark_core::FieldValue) -> String {
    match value {
        waymark_core::FieldValue::String(text) => text.clone(),
        waymark_core::FieldValue::Integer(number) => number.to_string(),
        waymark_core::FieldValue::Boolean(flag) => flag.to_string(),
        waymark_core::FieldValue::List(values) => {
            let rendered: Vec<String> = values.iter().map(render_field_value).collect();
            format!("[{}]", rendered.join(", "))
        }
    }
}

/// Write one JSON value plus a newline to stdout.
fn emit(value: &serde_json::Value) -> Result<(), WaymarkError> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    serde_json::to_writer(&mut handle, value).map_err(|source| {
        WaymarkError::internal(format!("cannot serialize the envelope: {source}"))
    })?;
    handle
        .write_all(b"\n")
        .map_err(|source| WaymarkError::io("stdout", "write", &source))?;
    Ok(())
}

/// Human narrative goes to stderr, always.
fn err_line(line: &str) {
    let stderr = std::io::stderr();
    let mut handle = stderr.lock();
    let _ = writeln!(handle, "{line}");
}

/// Report a failure in the shape the mode asks for.
///
/// JSON mode writes the failure envelope to stdout so a parser always receives
/// valid JSON; human mode writes prose to stderr. Neither changes the exit code,
/// which comes from the registry.
fn report_failure(error: &WaymarkError, json: bool) {
    if json {
        let _ = emit(&failure_envelope(error));
        return;
    }
    err_line(&format!("error: {}", error.message()));
    if let Some(location) = error.location() {
        err_line(&format!("  at {location}"));
    }
    if error.retryable() {
        err_line("  this failure is retryable");
    }
}
