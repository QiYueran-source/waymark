//! `waymark migrate version|schema`: explicit format and schema migration.
//!
//! Every arm is thin: the domain plans, revalidates, and writes, and the adapter
//! renders the same report either as one JSON envelope on stdout or as prose on
//! stderr. A dry run passes the same request with the dry-run flag, so the
//! adapter cannot accidentally implement a second, weaker check.

use clap::{ArgGroup, Args, Subcommand};
use waymark_core::config::FieldType;
use waymark_core::envelope::success_envelope;
use waymark_core::error::WaymarkError;
use waymark_core::model::FieldKey;
use waymark_core::ops::migrate::{self, MigrationReport, SchemaAction};
use waymark_core::ops::OpOptions;

use super::{emit, err_line, open_project};

/// Arguments of `waymark migrate`.
#[derive(Debug, Args)]
pub struct MigrateArgs {
    #[command(subcommand)]
    pub command: MigrateCommand,
}

/// The two migration kinds this phase exposes.
#[derive(Debug, Subcommand)]
pub enum MigrateCommand {
    /// Rewrite every older-generation document and raise the configuration
    /// generation.
    Version(VersionArgs),
    /// Remove one declared custom field, or add a required one with a default.
    Schema(SchemaArgs),
}

/// `migrate version --to <N>`.
#[derive(Debug, Args)]
pub struct VersionArgs {
    /// Target format generation; must be the generation this release writes.
    #[arg(long, value_name = "N")]
    pub to: u32,

    /// Perform the complete read-only check and report the affected files.
    #[arg(long)]
    pub dry_run: bool,
}

/// `migrate schema --remove-field <KEY>` or
/// `migrate schema --add-required-field <KEY> --type <TYPE> [--default <VALUE>]`.
#[derive(Debug, Args)]
#[command(group = ArgGroup::new("action").required(true).multiple(false))]
pub struct SchemaArgs {
    /// Delete this declaration and its key from every work item.
    #[arg(long, value_name = "KEY", group = "action")]
    pub remove_field: Option<String>,

    /// Declare this field as required and supply its default where missing.
    #[arg(long, value_name = "KEY", group = "action")]
    pub add_required_field: Option<String>,

    /// Declared value shape for `--add-required-field`.
    #[arg(long = "type", value_name = "TYPE", requires = "add_required_field")]
    pub ty: Option<String>,

    /// Default written wherever an item has no value.
    #[arg(long, value_name = "VALUE", requires = "add_required_field")]
    pub default: Option<String>,

    /// Perform the complete read-only check and report the affected files.
    #[arg(long)]
    pub dry_run: bool,
}

/// Run `waymark migrate`.
pub fn run(
    args: MigrateArgs,
    project: Option<&camino::Utf8Path>,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    match args.command {
        MigrateCommand::Version(args) => {
            let project = open_project(project)?;
            let options = if args.dry_run {
                options.clone().dry_run()
            } else {
                options.clone()
            };
            let report = migrate::migrate_version(&project, args.to, &options)?;
            render(&report, json)
        }
        MigrateCommand::Schema(args) => {
            let action = schema_action(&args)?;
            let project = open_project(project)?;
            let options = if args.dry_run {
                options.clone().dry_run()
            } else {
                options.clone()
            };
            let report = migrate::migrate_schema(&project, action, &options)?;
            render(&report, json)
        }
    }
}

/// Translate the schema flags into exactly one domain action.
fn schema_action(args: &SchemaArgs) -> Result<SchemaAction, WaymarkError> {
    match (&args.remove_field, &args.add_required_field) {
        (Some(key), None) => Ok(SchemaAction::RemoveField(field_key(key)?)),
        (None, Some(key)) => {
            let Some(ty) = &args.ty else {
                return Err(argument(
                    "type",
                    "--add-required-field needs --type <TYPE>".to_owned(),
                ));
            };
            let ty = FieldType::parse(ty).map_err(|reason| argument("type", reason))?;
            let default = match &args.default {
                Some(raw) => Some(
                    ty.parse_value(raw)
                        .map_err(|reason| argument("default", reason))?,
                ),
                None => None,
            };
            Ok(SchemaAction::AddRequiredField {
                key: field_key(key)?,
                ty,
                default,
            })
        }
        _ => Err(argument(
            "action",
            "exactly one of --remove-field or --add-required-field is required".to_owned(),
        )),
    }
}

fn field_key(literal: &str) -> Result<FieldKey, WaymarkError> {
    FieldKey::parse(literal).map_err(|error| argument("key", error.to_string()))
}

fn argument(name: &str, reason: String) -> WaymarkError {
    WaymarkError::validation(name, vec![waymark_core::ValidationIssue::new(name, reason)])
}

/// Render one report: envelope on stdout in JSON mode, prose on stderr
/// otherwise.
fn render(report: &MigrationReport, json: bool) -> Result<(), WaymarkError> {
    if json {
        emit(&success_envelope(report.data_json(), report.meta.clone()))?;
        return Ok(());
    }

    let state = if report.dry_run {
        "dry run; nothing written"
    } else if report.written {
        "written"
    } else {
        "nothing to do"
    };
    err_line(&format!("{}: {state}", report.description));
    for item in &report.affected {
        err_line(&format!("  {}  {}", item.id, item.path));
    }
    err_line(&format!(
        "  {} work-item file(s); configuration {}; revision {} -> {}",
        report.count,
        if report.config_changed {
            "changed"
        } else {
            "unchanged"
        },
        report.previous_revision,
        report.revision
    ));
    for warning in &report.meta.warnings {
        err_line(&format!(
            "warning: {}: {}",
            warning.code.as_str(),
            warning.message
        ));
    }
    for target in &report.targets {
        if target.outcome == "failed" {
            err_line(&format!(
                "  failed: {}{}",
                target.path,
                target
                    .error
                    .as_ref()
                    .map(|error| format!(": {error}"))
                    .unwrap_or_default()
            ));
        }
    }
    Ok(())
}
