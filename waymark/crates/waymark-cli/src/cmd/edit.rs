//! `waymark title set`, `waymark body replace`, and `waymark field update`.
//!
//! The three ordinary-edit surfaces share one shape — one item, one `--dry-run`,
//! and the global `--json` — because they are three views of the same core
//! operation family. Every rule lives in [`waymark_core::ops::edit`]; this module
//! only translates arguments into domain values and the outcome into output.
//!
//! # Body input
//!
//! `--body` and `--body-file` are mutually exclusive. `-` in either one means
//! "read the body from stdin to EOF"; there is no interactive editor. An empty
//! `--body ""` or an empty file clears the body, and omitting both leaves it
//! untouched. The file is read as UTF-8; a non-UTF-8 file is an I/O failure, not
//! a silently lossy read.
//!
//! # Field values
//!
//! A `--set key=value` value is resolved against the declared type, never guessed
//! from the literal alone:
//!
//! - `integer` takes decimal digits with an optional leading `-` (`0x10`, `+3`,
//!   `3.0`, and `1_000` are rejected);
//! - `boolean` takes exactly `true` or `false`;
//! - `list:*` takes a JSON array whose elements are checked against the element
//!   type by the core declaration;
//! - `string` takes a JSON string literal when the token starts with `"`, and
//!   otherwise the bare token verbatim. A token that is a JSON array or object is
//!   rejected: a structured value for a string field is a mistake, not content.
//!
//! Duplicate and conflicting keys are rejected before the write, as are unknown
//! keys, `null` values, and clearing a required field — all through the core
//! operation, so the CLI cannot accept something the domain refuses.

use std::io::Read;

use camino::Utf8PathBuf;
use clap::{Args, Subcommand};
use serde_json::Value as Json;
use waymark_core::config::{FieldDecl, FieldType};
use waymark_core::envelope::success_envelope;
use waymark_core::error::{ValidationIssue, WaymarkError};
use waymark_core::model::FieldKey;
use waymark_core::ops::edit::{self, EditOutcome, FieldChange, FieldChangeEntry, FieldOutcome};
use waymark_core::ops::OpOptions;
use waymark_core::{FieldValue, Project};

use super::{emit, err_line, item_json};

/// Arguments of `waymark title`.
#[derive(Debug, Args)]
pub struct TitleArgs {
    #[command(subcommand)]
    pub command: TitleCommand,
}

/// The title family.
#[derive(Debug, Subcommand)]
pub enum TitleCommand {
    /// Replace the title of one item.
    Set(SetTitleArgs),
}

/// Arguments of `waymark title set`.
#[derive(Debug, Args)]
pub struct SetTitleArgs {
    /// Stable ID of the item.
    #[arg(value_name = "ID")]
    pub id: String,

    /// The new title. Surrounding whitespace is trimmed; internal whitespace is
    /// kept. A title that starts with `-` needs `--` before it.
    #[arg(value_name = "TITLE")]
    pub title: String,

    /// Report the change without writing it.
    #[arg(long)]
    pub dry_run: bool,
}

/// Arguments of `waymark body`.
#[derive(Debug, Args)]
pub struct BodyArgs {
    #[command(subcommand)]
    pub command: BodyCommand,
}

/// The body family.
#[derive(Debug, Subcommand)]
pub enum BodyCommand {
    /// Replace, or with an empty value clear, the body of one item.
    Replace(ReplaceBodyArgs),
}

/// Arguments of `waymark body replace`.
#[derive(Debug, Args)]
pub struct ReplaceBodyArgs {
    /// Stable ID of the item.
    #[arg(value_name = "ID")]
    pub id: String,

    /// The new body, or `-` to read it from stdin.
    ///
    /// An empty value clears the body. Mutually exclusive with `--body-file`.
    #[arg(long, value_name = "TEXT", conflicts_with = "body_file")]
    pub body: Option<String>,

    /// Read the new body from this file, or `-` for stdin.
    #[arg(long, value_name = "PATH", conflicts_with = "body")]
    pub body_file: Option<Utf8PathBuf>,

    /// Report the change without writing it.
    #[arg(long)]
    pub dry_run: bool,
}

/// Arguments of `waymark field`.
#[derive(Debug, Args)]
pub struct FieldArgs {
    #[command(subcommand)]
    pub command: FieldCommand,
}

/// The custom-field family.
#[derive(Debug, Subcommand)]
pub enum FieldCommand {
    /// Set and/or clear custom fields in one validated change set.
    Update(UpdateFieldsArgs),
}

/// Arguments of `waymark field update`.
#[derive(Debug, Args)]
pub struct UpdateFieldsArgs {
    /// Stable ID of the item.
    #[arg(value_name = "ID")]
    pub id: String,

    /// Set a field to a value, as `KEY=VALUE`. Repeatable; may be mixed with
    /// `--clear`.
    #[arg(long = "set", value_name = "KEY=VALUE", action = clap::ArgAction::Append)]
    pub set: Vec<String>,

    /// Clear a field. Repeatable; may be mixed with `--set`.
    #[arg(long = "clear", value_name = "KEY", action = clap::ArgAction::Append)]
    pub clear: Vec<String>,

    /// Report the change without writing it.
    #[arg(long)]
    pub dry_run: bool,
}

/// Run `waymark title`.
pub fn run_title(
    project: &Project,
    args: TitleArgs,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    match args.command {
        TitleCommand::Set(args) => {
            let options = dry_run(options, args.dry_run);
            let outcome = edit::set_title(project, &args.id, &args.title, &options)?;
            render_edit(&outcome, json, describe_title)
        }
    }
}

/// Run `waymark body`.
pub fn run_body(
    project: &Project,
    args: BodyArgs,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    match args.command {
        BodyCommand::Replace(args) => {
            let body = body_input(&args)?;
            let options = dry_run(options, args.dry_run);
            let outcome = edit::replace_body(project, &args.id, body.as_deref(), &options)?;
            render_edit(&outcome, json, describe_body)
        }
    }
}

/// Run `waymark field`.
pub fn run_field(
    project: &Project,
    args: FieldArgs,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    match args.command {
        FieldCommand::Update(args) => {
            let changes = field_changes(project, &args)?;
            let options = dry_run(options, args.dry_run);
            let outcome = edit::update_custom_fields(project, &args.id, &changes, &options)?;
            render_fields(&outcome, json)
        }
    }
}

fn dry_run(options: &OpOptions, requested: bool) -> OpOptions {
    if requested {
        options.clone().dry_run()
    } else {
        options.clone()
    }
}

/// Resolve the body argument pair into the core's `Option<String>`.
///
/// `None` means "leave the body alone" and is possible only when neither flag was
/// given. `-` reads stdin in either form.
fn body_input(args: &ReplaceBodyArgs) -> Result<Option<String>, WaymarkError> {
    if let Some(text) = &args.body {
        if text == "-" {
            return read_stdin().map(Some);
        }
        return Ok(Some(text.clone()));
    }
    if let Some(path) = &args.body_file {
        if path.as_str() == "-" {
            return read_stdin().map(Some);
        }
        let text = std::fs::read_to_string(path)
            .map_err(|source| WaymarkError::io(path.to_string(), "read", &source))?;
        return Ok(Some(text));
    }
    Ok(None)
}

fn read_stdin() -> Result<String, WaymarkError> {
    let mut buffer = String::new();
    std::io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|source| WaymarkError::io("stdin", "read", &source))?;
    Ok(buffer)
}

/// Parse the `--set`/`--clear` arguments into one core change set.
///
/// Duplicate and conflicting keys are reported together before any value is
/// parsed, so a contradictory command line is never half-translated.
fn field_changes(
    project: &Project,
    args: &UpdateFieldsArgs,
) -> Result<Vec<FieldChangeEntry>, WaymarkError> {
    let mut issues: Vec<ValidationIssue> = Vec::new();
    let mut entries: Vec<FieldChangeEntry> = Vec::new();
    let mut directions: std::collections::BTreeMap<&str, Vec<&'static str>> =
        std::collections::BTreeMap::new();

    for raw in &args.set {
        let Some((key, value)) = raw.split_once('=') else {
            issues.push(ValidationIssue::new(
                format!("set.{raw}"),
                "a set argument must be `KEY=VALUE`",
            ));
            continue;
        };
        directions.entry(key).or_default().push("set");
        entries.push(FieldChangeEntry::set(
            key,
            FieldValue::String(value.to_owned()),
        ));
    }
    for key in &args.clear {
        directions.entry(key).or_default().push("clear");
        entries.push(FieldChangeEntry::clear(key));
    }

    for (key, kinds) in &directions {
        if kinds.len() > 1 {
            issues.push(ValidationIssue::new(
                format!("custom_fields.{key}"),
                if kinds.contains(&"set") && kinds.contains(&"clear") {
                    "the key is both set and cleared; use exactly one direction".to_owned()
                } else {
                    format!("the key appears {} times; name it once", kinds.len())
                },
            ));
        }
    }
    if !issues.is_empty() {
        return Err(WaymarkError::validation("custom_fields", issues).at_field("custom_fields"));
    }

    // Resolve every remaining `set` value against the declared type. An unknown
    // or unparseable key keeps its raw text: the core rejects the key itself, and
    // that message — not a guessed type error — is the one the caller needs.
    let declared = &project.config().fields;
    let mut resolved: Vec<FieldChangeEntry> = Vec::new();
    for entry in entries {
        let FieldChange::Set(FieldValue::String(raw)) = &entry.change else {
            resolved.push(entry);
            continue;
        };
        let key = entry.key;
        let declaration = FieldKey::parse(&key)
            .ok()
            .and_then(|key| declared.get(&key).cloned());
        let Some(declaration) = declaration else {
            resolved.push(FieldChangeEntry::set(key, FieldValue::String(raw.clone())));
            continue;
        };
        match parse_field_value(&declaration, raw) {
            Ok(value) => resolved.push(FieldChangeEntry::set(key, value)),
            Err(reason) => {
                issues.push(ValidationIssue::new(format!("custom_fields.{key}"), reason));
            }
        }
    }
    if !issues.is_empty() {
        return Err(WaymarkError::validation("custom_fields", issues).at_field("custom_fields"));
    }
    Ok(resolved)
}

/// Resolve one raw `--set`/`--field` token against the declared field type.
///
/// Shared with `create` so a field value typed on either command line has the
/// same text grammar and the same parse errors.
pub(crate) fn parse_field_value(declaration: &FieldDecl, raw: &str) -> Result<FieldValue, String> {
    match &declaration.ty {
        FieldType::String => parse_string(raw),
        FieldType::Integer => parse_integer(raw).map(FieldValue::Integer),
        FieldType::Boolean => match raw {
            "true" => Ok(FieldValue::Boolean(true)),
            "false" => Ok(FieldValue::Boolean(false)),
            other => Err(format!("expected `true` or `false`, found {other:?}")),
        },
        FieldType::List(element) => parse_list(element, raw),
    }
}

/// A string is either a JSON string literal or a bare token taken verbatim.
fn parse_string(raw: &str) -> Result<FieldValue, String> {
    if raw.starts_with('"') {
        return match serde_json::from_str::<Json>(raw) {
            Ok(Json::String(text)) => Ok(FieldValue::String(text)),
            Ok(_) => Err("a string value must be a JSON string literal".to_owned()),
            Err(error) => Err(format!("not a valid JSON string literal: {error}")),
        };
    }
    // A structured token for a string field is a mistake, not content.
    match serde_json::from_str::<Json>(raw) {
        Ok(Json::Array(_) | Json::Object(_)) => Err(
            "a string value may not be a JSON array or object; quote it to keep it as text"
                .to_owned(),
        ),
        _ => Ok(FieldValue::String(raw.to_owned())),
    }
}

/// Decimal integer syntax only: optional `-`, then at least one ASCII digit.
fn parse_integer(raw: &str) -> Result<i64, String> {
    let digits = raw.strip_prefix('-').unwrap_or(raw);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("expected a decimal integer, found {raw:?}"));
    }
    raw.parse::<i64>()
        .map_err(|_| format!("{raw:?} is outside the range of a 64-bit integer"))
}

/// A list value is a JSON array, recursively checked against the element type.
fn parse_list(element: &FieldType, raw: &str) -> Result<FieldValue, String> {
    let parsed: Json = serde_json::from_str(raw)
        .map_err(|error| format!("expected a JSON array, found {raw:?}: {error}"))?;
    let Json::Array(items) = parsed else {
        return Err(format!("expected a JSON array, found {raw:?}"));
    };
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        values.push(json_to_field_value(element, item)?);
    }
    Ok(FieldValue::List(values))
}

/// Convert one JSON array element to the declared element type, strictly.
fn json_to_field_value(element: &FieldType, value: Json) -> Result<FieldValue, String> {
    match (element, value) {
        (FieldType::String, Json::String(text)) => Ok(FieldValue::String(text)),
        (FieldType::Integer, Json::Number(number)) => number
            .as_i64()
            .map(FieldValue::Integer)
            .ok_or_else(|| format!("{} is not a whole number", number)),
        (FieldType::Boolean, Json::Bool(flag)) => Ok(FieldValue::Boolean(flag)),
        (FieldType::List(_), _) => Err("nested lists are not valid custom-field values".to_owned()),
        (element, other) => Err(format!(
            "expected a `{}` element, found {}",
            element.as_str(),
            other
        )),
    }
}

/// The `data` block common to title and body edits.
fn render_edit(
    outcome: &EditOutcome,
    json: bool,
    describe: fn(&EditOutcome) -> String,
) -> Result<(), WaymarkError> {
    if json {
        emit(&success_envelope(
            serde_json::json!({
                "id": outcome.item.id.as_str(),
                "path": outcome.path.to_string(),
                "changed": outcome.changed,
                "written": outcome.written,
                "item": item_json(&outcome.item, &outcome.path),
            }),
            outcome.meta.clone(),
        ))?;
        return Ok(());
    }

    let prefix = if outcome.written { "" } else { "would be " };
    let verb = if outcome.changed {
        "updated"
    } else {
        "unchanged"
    };
    err_line(&format!(
        "{prefix}{verb} {} ({})",
        outcome.item.id,
        describe(outcome)
    ));
    warnings(&outcome.meta.warnings);
    Ok(())
}

/// The `data` block for a custom-field update, which also names the changed
/// fields.
fn render_fields(outcome: &FieldOutcome, json: bool) -> Result<(), WaymarkError> {
    let changed_fields: Vec<&str> = outcome
        .changed_fields
        .iter()
        .map(FieldKey::as_str)
        .collect();
    if json {
        emit(&success_envelope(
            serde_json::json!({
                "id": outcome.item.id.as_str(),
                "path": outcome.path.to_string(),
                "changed": outcome.changed,
                "written": outcome.written,
                "changed_fields": changed_fields,
                "item": item_json(&outcome.item, &outcome.path),
            }),
            outcome.meta.clone(),
        ))?;
        return Ok(());
    }

    let prefix = if outcome.written { "" } else { "would be " };
    let verb = if outcome.changed {
        "updated"
    } else {
        "unchanged"
    };
    let fields = if changed_fields.is_empty() {
        "no fields".to_owned()
    } else {
        changed_fields.join(", ")
    };
    err_line(&format!(
        "{prefix}{verb} {} custom fields ({fields})",
        outcome.item.id
    ));
    warnings(&outcome.meta.warnings);
    Ok(())
}

/// Render the advisory warnings, one per line, on stderr.
fn warnings(items: &[waymark_core::envelope::Warning]) {
    for warning in items {
        err_line(&format!("  {}: {}", warning.code, warning.message));
    }
}

fn describe_title(outcome: &EditOutcome) -> String {
    format!("title {:?}", outcome.item.title)
}

fn describe_body(outcome: &EditOutcome) -> String {
    if outcome.item.body.is_empty() {
        "body is now empty".to_owned()
    } else {
        format!("body is {} bytes", outcome.item.body.len())
    }
}
