//! `waymark create`: create one work item.
//!
//! The command line mirrors the catalog's complete input: the required title,
//! category, and owner; an optional creator; a body or body file; repeatable
//! declared custom fields; and the initial parent, dependencies, and soft links.
//! All of it is one atomic domain operation — see
//! [`waymark_core::ops::create`] — so `--dry-run` performs the same checks and
//! writes nothing.

use std::io::Read;

use camino::Utf8PathBuf;
use clap::Args;
use waymark_core::envelope::success_envelope;
use waymark_core::error::{ValidationIssue, WaymarkError};
use waymark_core::model::{FieldKey, FieldValue};
use waymark_core::ops::create::CreateRequest;
use waymark_core::ops::{create, OpOptions};
use waymark_core::Project;

use super::{emit, err_line};
use waymark_core::view::item_json;

/// Arguments of `waymark create`.
#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Short single-line label.
    #[arg(long, value_name = "TEXT")]
    pub title: String,

    /// Category key the item belongs to; must already be declared.
    #[arg(long, value_name = "KEY")]
    pub category: String,

    /// Accountable identity.
    #[arg(long, value_name = "NAME")]
    pub owner: String,

    /// Provenance identity. Defaults to the acting operator when omitted.
    #[arg(long, value_name = "NAME")]
    pub creator: Option<String>,

    /// Markdown body, or `-` to read it from stdin.
    #[arg(long, value_name = "TEXT", conflicts_with = "body_file")]
    pub body: Option<String>,

    /// Read the body from this file, or `-` for stdin.
    #[arg(long, value_name = "PATH", conflicts_with = "body")]
    pub body_file: Option<Utf8PathBuf>,

    /// Set a declared custom field, as `KEY=VALUE`. Repeatable.
    #[arg(long = "field", value_name = "KEY=VALUE", action = clap::ArgAction::Append)]
    pub field: Vec<String>,

    /// Stable ID of the parent the item starts under.
    #[arg(long, value_name = "ID")]
    pub parent: Option<String>,

    /// Stable ID the item waits for. Repeatable.
    #[arg(long = "dependency", value_name = "ID", action = clap::ArgAction::Append)]
    pub dependency: Vec<String>,

    /// Stable ID to soft-link to. Repeatable; the link is stored on the new item,
    /// and on the other side only with `--cascade`.
    #[arg(long = "link", value_name = "ID", action = clap::ArgAction::Append)]
    pub link: Vec<String>,

    /// Consent to a cross-item write: also record the link on the other side,
    /// which requires you to own it. Defaults to off.
    #[arg(long)]
    pub cascade: bool,

    /// Validate and report what would be written, without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// Run `waymark create`.
pub fn run(
    project: &Project,
    args: CreateArgs,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    let body = read_body(args.body.as_deref(), args.body_file.as_deref())?;
    let fields = field_values(project, &args.field)?;

    let mut request = CreateRequest::new(args.title, args.category, args.owner);
    if let Some(creator) = args.creator {
        request = request.with_creator(creator);
    }
    if let Some(body) = body {
        request = request.with_body(body);
    }
    for (key, value) in fields {
        request = request.with_field(key, value);
    }
    if let Some(parent) = args.parent {
        request = request.with_parent(parent);
    }
    for dependency in args.dependency {
        request = request.with_dependency(dependency);
    }
    for link in args.link {
        request = request.with_soft_link(link);
    }
    let mut options = options.clone();
    if args.dry_run {
        options = options.dry_run();
    }
    if args.cascade {
        options.cascade = true;
    }

    let created = create::create(project, request, &options)?;

    if json {
        emit(&success_envelope(
            serde_json::json!({
                "id": created.item.id.as_str(),
                "path": created.path.to_string(),
                "written": created.written,
                "item": item_json(&created.item, &created.path),
                "partners": created.partners.iter().map(|partner| {
                    item_json(&partner.item, &partner.path)
                }).collect::<Vec<_>>(),
                "untouched": waymark_core::view::leftover_partners_json(&created.untouched),
                "fingerprints": created.fingerprints.iter().map(|fingerprint| fingerprint.as_str()).collect::<Vec<_>>(),
            }),
            created.meta.clone(),
        ))?;
    } else if created.written {
        err_line(&format!("created {}", created.item.id));
        err_line(&format!("  {}", created.path));
        for partner in &created.partners {
            err_line(&format!("  updated {}", partner.item.id));
        }
    } else {
        err_line(&format!("would create {}", created.item.id));
        err_line(&format!("  {}", created.path));
        for partner in &created.partners {
            err_line(&format!("  would update {}", partner.item.id));
        }
    }
    for partner in &created.untouched {
        err_line(&format!(
            "  {} left alone (it did not record the link)",
            partner.item.id
        ));
    }
    for warning in &created.meta.warnings {
        err_line(&format!("  {}: {}", warning.code, warning.message));
    }
    Ok(())
}

/// Read the body from `--body`/`--body-file`, where `-` means stdin.
///
/// The two flags are mutually exclusive at the argument layer, so this only
/// decides where the one value comes from.
fn read_body(
    body: Option<&str>,
    body_file: Option<&camino::Utf8Path>,
) -> Result<Option<String>, WaymarkError> {
    match (body, body_file) {
        (Some("-"), _) => read_stdin().map(Some),
        (Some(text), _) => Ok(Some(text.to_owned())),
        (None, Some(path)) if path.as_str() == "-" => read_stdin().map(Some),
        (None, Some(path)) => std::fs::read_to_string(path)
            .map(Some)
            .map_err(|source| WaymarkError::io(path.to_string(), "read", &source)),
        (None, None) => Ok(None),
    }
}

fn read_stdin() -> Result<String, WaymarkError> {
    let mut buffer = String::new();
    std::io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|source| WaymarkError::io("stdin", "read", &source))?;
    Ok(buffer)
}

/// Parse every `--field KEY=VALUE` token against the declared field type.
///
/// This is the shared `FieldType::parse_value` grammar, the same one
/// `field update --set` and `migrate schema --default` use. An undeclared key
/// keeps its raw text so the domain reports "not declared" instead of guessing a
/// type from it; the domain still owns the declaration and required-field rules.
fn field_values(
    project: &Project,
    raw_fields: &[String],
) -> Result<Vec<(FieldKey, FieldValue)>, WaymarkError> {
    let declared = &project.config().fields;
    let mut issues: Vec<ValidationIssue> = Vec::new();
    let mut fields: Vec<(FieldKey, FieldValue)> = Vec::new();

    for raw in raw_fields {
        let Some((key, value)) = raw.split_once('=') else {
            issues.push(ValidationIssue::new(
                format!("field.{raw}"),
                "a field argument must be `KEY=VALUE`",
            ));
            continue;
        };
        let key = match FieldKey::parse(key) {
            Ok(key) => key,
            Err(error) => {
                issues.push(ValidationIssue::new(
                    format!("custom_fields.{key}"),
                    format!("`{key}` is not a valid custom-field key: {error}"),
                ));
                continue;
            }
        };
        let Some(declaration) = declared.get(&key) else {
            fields.push((key, FieldValue::String(value.to_owned())));
            continue;
        };
        match declaration.ty.parse_value(value) {
            Ok(value) => fields.push((key, value)),
            Err(reason) => {
                issues.push(ValidationIssue::new(format!("custom_fields.{key}"), reason));
            }
        }
    }

    if issues.is_empty() {
        Ok(fields)
    } else {
        Err(WaymarkError::validation("custom_fields", issues).at_field("custom_fields"))
    }
}
