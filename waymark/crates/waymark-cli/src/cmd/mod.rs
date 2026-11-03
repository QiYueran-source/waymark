//! Shared adapter plumbing: where output goes, how a failure is rendered, and
//! the two pieces of context every command needs.
//!
//! One command family per module; this module owns everything a family would
//! otherwise duplicate.

pub mod agent_doc;
pub mod archive;
pub mod config;
pub mod create;
pub mod edit;
pub mod git;
pub mod init;
pub mod link;
pub mod list;
pub mod migrate;
pub mod parent;
pub mod project;
pub mod query;
pub mod relationship;
pub mod show;
pub mod status;
pub mod tree;

use std::io::Write;

use camino::Utf8PathBuf;
use serde_json::Value;
use waymark_core::envelope::{adapter_failure_envelope, failure_envelope};
use waymark_core::error::WaymarkError;
use waymark_core::lock::LockWait;
use waymark_core::{Project, WorkItem};

/// The process exit code for a plain success.
pub const EXIT_OK: u8 = 0;

/// The adapter-level usage error code: bad flags, unknown subcommands, an
/// unparseable command line.
///
/// Deliberately **not** a registry `Code`. No domain operation can produce it, so
/// registering it would let a domain caller claim a recovery action that does not
/// exist; the adapter owns both the code and its exit.
pub const USAGE_ERROR: &str = "USAGE_ERROR";

/// The process exit code for [`USAGE_ERROR`].
pub const USAGE_EXIT: u8 = 1;

/// The process exit code for a failure.
///
/// This is the *only* place an exit code is decided, and it reads the registry
/// through [`WaymarkError::exit_class`]. A finding-only code has no exit class,
/// which cannot happen through a domain operation; if it does, the fault is ours
/// and the internal class is the honest answer.
pub fn exit_code(error: &WaymarkError) -> u8 {
    match error.exit_class() {
        Some(class) => u8::try_from(class.as_i32()).unwrap_or(EXIT_INTERNAL),
        None => EXIT_INTERNAL,
    }
}

/// Exit class 7, used when a failure carries no exit class at all.
const EXIT_INTERNAL: u8 = 7;

/// Whether the raw command line asked for JSON.
///
/// A parse failure happens before the flag is known, so the adapter reads the
/// token it would have parsed. The envelope shape matters even for a usage error:
/// a parser that asked for JSON must receive JSON.
pub fn json_requested(args: &[std::ffi::OsString]) -> bool {
    args.iter()
        .any(|argument| argument == std::ffi::OsStr::new("--json"))
}

/// Report a usage failure: the code, the exit, and the mode the caller asked for.
///
/// `clap` has already rendered its own message to stderr; in JSON mode a failure
/// envelope goes to stdout so a parser always receives one JSON value.
pub fn report_usage_error(error: &clap::Error, json: bool) {
    let rendered = error.to_string();
    let mut lines = rendered.lines();
    let summary = lines
        .next()
        .unwrap_or("invalid command line")
        .trim()
        .trim_start_matches("error: ")
        .to_owned();

    if json {
        // `clap`'s own narrative stays on stderr; stdout carries only the
        // envelope, so a parser never sees prose.
        let _ = error.print();
        let envelope =
            adapter_failure_envelope(USAGE_ERROR, &summary, serde_json::json!({}), None, false);
        let _ = emit(&envelope);
        return;
    }

    err_line(&format!("error: {USAGE_ERROR}: {summary}"));
    for line in lines {
        err_line(line);
    }
}

/// The directory the command was invoked from.
pub fn invocation_dir() -> Result<Utf8PathBuf, WaymarkError> {
    let cwd = std::env::current_dir()
        .map_err(|source| WaymarkError::io("the working directory", "read", &source))?;
    Utf8PathBuf::from_path_buf(cwd)
        .map_err(|_| WaymarkError::internal("the working directory is not UTF-8"))
}

/// Open the project named by `--project`, or discover it from the invocation
/// directory.
pub fn open_project(explicit: Option<&camino::Utf8Path>) -> Result<Project, WaymarkError> {
    match explicit {
        Some(dir) => Project::discover(dir),
        None => Project::discover(&invocation_dir()?),
    }
}

/// Translate `--lock-timeout-ms` into a lock wait policy.
pub fn lock_wait(milliseconds: Option<u64>) -> LockWait {
    match milliseconds {
        Some(0) => LockWait::immediate(),
        Some(value) => LockWait::new(std::time::Duration::from_millis(value)),
        None => LockWait::default_policy(),
    }
}

/// The machine form of one item: the persisted fields, no prose.
pub fn item_json(item: &WorkItem, path: &camino::Utf8Path) -> Value {
    let mut value = item_fields(item);
    if let Some(map) = value.as_object_mut() {
        map.insert("path".to_owned(), serde_json::json!(path.to_string()));
    }
    value
}

/// The persisted fields of one item, without the file it happens to live in.
///
/// `show` names the document; a page of many items does not, because the path is
/// a fact about one file and a listing is not about files.
pub fn item_fields(item: &WorkItem) -> Value {
    serde_json::json!({
        "schema": item.schema,
        "needs_migration": item.needs_migration(),
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
    })
}

/// Render a custom field for human output.
pub fn render_field_value(value: &waymark_core::FieldValue) -> String {
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
pub fn emit(value: &Value) -> Result<(), WaymarkError> {
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
pub fn err_line(line: &str) {
    let stderr = std::io::stderr();
    let mut handle = stderr.lock();
    let _ = writeln!(handle, "{line}");
}

/// Report a failure in the shape the mode asks for.
///
/// JSON mode writes the failure envelope to stdout so a parser always receives
/// valid JSON; human mode writes prose to stderr. Neither changes the exit code,
/// which comes from the registry.
pub fn report_failure(error: &WaymarkError, json: bool) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use waymark_core::error::Code;
    use waymark_core::WorkItemId;

    fn id() -> WorkItemId {
        WorkItemId::parse("wm-000000000001").unwrap()
    }

    #[test]
    fn the_exit_code_comes_from_the_registry_and_nothing_else() {
        assert_eq!(exit_code(&WaymarkError::item_not_found(&id())), 2);
        assert_eq!(
            exit_code(&WaymarkError::parse("/p/a.md", "bad frontmatter")),
            3
        );
        assert_eq!(
            exit_code(&WaymarkError::write_conflict(&id(), "/p/a.md", "aa", "bb")),
            4
        );
        assert_eq!(exit_code(&WaymarkError::project_not_found("/nowhere")), 5);
        assert_eq!(exit_code(&WaymarkError::unsupported_version("3", "1")), 6);
        assert_eq!(exit_code(&WaymarkError::internal("boom")), 7);
    }

    #[test]
    fn a_finding_only_code_used_as_a_failure_falls_back_to_the_internal_class() {
        // Defensive: a domain operation cannot produce one of these, but if a
        // caller does, the process must not exit 0 on a failure.
        let error = WaymarkError::owner_mismatch(&id(), "frank", "alice");
        assert_eq!(error.code().exit_class(), None);
        assert_eq!(exit_code(&error), 7);
    }

    #[test]
    fn a_warning_is_never_a_failure_code() {
        assert_eq!(Code::OwnerMismatch.exit_class(), None);
        assert_eq!(Code::LockHeld.exit_class(), None);
        assert_eq!(Code::MigrationAvailable.exit_class(), None);
        assert_eq!(Code::NotAGitRepository.exit_class(), None);
        assert_eq!(Code::DanglingReference.exit_class(), None);
        assert_eq!(Code::TimestampIncoherent.exit_class(), None);
    }
}
