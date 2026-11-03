//! `waymark config show|validate|set|add|remove`: the configuration surface.
//!
//! Every arm is thin: the domain decides what is allowed and what changed, the
//! adapter decides how a person or a parser reads it. Human output is prose on
//! stderr; `--json` is one envelope on stdout. No arm re-implements a rule.

use clap::{Args, Subcommand};
use serde_json::Value;
use waymark_core::config::FieldType;
use waymark_core::envelope::{success_envelope, Finding, Meta};
use waymark_core::error::WaymarkError;
use waymark_core::model::{CategoryKey, FieldKey};
use waymark_core::ops::config::{self, ConfigOutcome, FieldAttribute, Scalar};
use waymark_core::ops::OpOptions;
use waymark_core::{Location, Project};

use super::{emit, err_line, invocation_dir, open_project};

/// Arguments of `waymark config`.
#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

/// The `config` subcommands.
#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print the effective configuration.
    Show,
    /// Report every problem in the configuration.
    Validate,
    /// Set a scalar, or an attribute with a subcommand.
    Set(SetArgs),
    /// Add a category or a custom field.
    Add(AddArgs),
    /// Remove an unused category.
    Remove(RemoveArgs),
}

/// `config set <name> <value>`, `config set category …`, `config set field …`.
#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
pub struct SetArgs {
    #[command(subcommand)]
    pub command: Option<SetCommand>,

    /// Scalar name: operator, timezone, cache.enabled, cache.capacity.
    #[arg(value_name = "NAME", required = true)]
    pub name: Option<String>,

    /// New value. An empty `operator` clears the key.
    #[arg(value_name = "VALUE", required = true)]
    pub value: Option<String>,

    /// Validate and report the change without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// What `config set` may address besides a scalar.
#[derive(Debug, Subcommand)]
pub enum SetCommand {
    /// Change a category's display name.
    Category(SetCategoryArgs),
    /// Change one attribute of a declared custom field.
    Field(SetFieldArgs),
}

/// `config set category <key> --name <name>`.
#[derive(Debug, Args)]
pub struct SetCategoryArgs {
    /// Category key.
    #[arg(value_name = "KEY")]
    pub key: String,
    /// New display name.
    #[arg(long, value_name = "NAME")]
    pub name: String,
    /// Validate and report the change without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// `config set field <key> <attribute> <value>`.
#[derive(Debug, Args)]
pub struct SetFieldArgs {
    /// Custom-field key.
    #[arg(value_name = "KEY")]
    pub key: String,
    /// Attribute: default, or required.
    #[arg(value_name = "ATTRIBUTE")]
    pub attribute: String,
    /// New value. `default null` clears the default.
    #[arg(value_name = "VALUE")]
    pub value: String,
    /// Validate and report the change without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// `config add category|field`.
#[derive(Debug, Args)]
pub struct AddArgs {
    #[command(subcommand)]
    pub command: AddCommand,
}

/// The two nouns `config add` accepts.
#[derive(Debug, Subcommand)]
pub enum AddCommand {
    /// Declare a category directory.
    Category(AddCategoryArgs),
    /// Declare an optional custom field.
    Field(AddFieldArgs),
}

/// `config add category <key> <path>`.
#[derive(Debug, Args)]
pub struct AddCategoryArgs {
    /// Category key: lowercase letters, digits, and hyphens.
    #[arg(value_name = "KEY")]
    pub key: String,
    /// Repository-relative directory for this category's documents.
    #[arg(value_name = "PATH")]
    pub path: String,
    /// Human-readable name. Defaults to the key with its first letter upper-cased.
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,
    /// Validate and report the change without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// `config add field <key> <type>`.
#[derive(Debug, Args)]
pub struct AddFieldArgs {
    /// Custom-field key: lowercase ASCII `snake_case`.
    #[arg(value_name = "KEY")]
    pub key: String,
    /// Declared type: string, integer, boolean, or list:<scalar>.
    #[arg(value_name = "TYPE")]
    pub ty: String,
    /// Declare the field as required. Refused: that is a migration.
    #[arg(long)]
    pub required: bool,
    /// Static default, parsed against the declared type.
    #[arg(long, value_name = "VALUE")]
    pub default: Option<String>,
    /// Validate and report the change without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// `config remove category <key>`.
#[derive(Debug, Args)]
pub struct RemoveArgs {
    #[command(subcommand)]
    pub command: RemoveCommand,
}

/// `config remove category`.
#[derive(Debug, Subcommand)]
pub enum RemoveCommand {
    /// Remove a category no work item uses.
    Category(RemoveCategoryArgs),
}

/// `config remove category <key>`.
#[derive(Debug, Args)]
pub struct RemoveCategoryArgs {
    /// Category key.
    #[arg(value_name = "KEY")]
    pub key: String,
    /// Check and report without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// Run `waymark config …`.
pub fn run(
    args: ConfigArgs,
    project: Option<&camino::Utf8Path>,
    options: &OpOptions,
    json: bool,
) -> Result<(), WaymarkError> {
    match args.command {
        ConfigCommand::Show => {
            let project = open_project(project)?;
            render_config(&config::show(&project)?, json)
        }
        ConfigCommand::Validate => {
            // Validation must work even when the document cannot be opened as a
            // project, so the file is located without parsing it.
            let path = match project {
                Some(dir) => config::locate_config_path(dir)?,
                None => config::locate_config_path(&invocation_dir()?)?,
            };
            render_findings(&config::validate_config_path(&path), json)
        }
        ConfigCommand::Set(args) => match args.command {
            Some(SetCommand::Category(args)) => {
                let project = open_project(project)?;
                capture(project, options, args.dry_run, |project, options| {
                    config::set_category_name(
                        project,
                        &category_key(&args.key)?,
                        &args.name,
                        options,
                    )
                })
                .and_then(|outcome| render_outcome(&outcome, json, "set category"))
            }
            Some(SetCommand::Field(args)) => {
                let project = open_project(project)?;
                capture(project, options, args.dry_run, |project, options| {
                    config::set_field(
                        project,
                        &field_key(&args.key)?,
                        FieldAttribute::parse(&args.attribute)?,
                        &args.value,
                        options,
                    )
                })
                .and_then(|outcome| render_outcome(&outcome, json, "set field"))
            }
            None => {
                let project = open_project(project)?;
                let name = args.name.unwrap_or_default();
                let value = args.value.unwrap_or_default();
                let scalar = Scalar::parse(&name)?;
                capture(project, options, args.dry_run, |project, options| {
                    config::set_scalar(project, scalar, &value, options)
                })
                .and_then(|outcome| {
                    render_outcome(&outcome, json, &format!("set {}", scalar.name()))
                })
            }
        },
        ConfigCommand::Add(args) => match args.command {
            AddCommand::Category(args) => {
                let project = open_project(project)?;
                capture(project, options, args.dry_run, |project, options| {
                    config::add_category(
                        project,
                        &category_key(&args.key)?,
                        &args.path,
                        args.name.as_deref(),
                        options,
                    )
                })
                .and_then(|outcome| render_outcome(&outcome, json, "add category"))
            }
            AddCommand::Field(args) => {
                let project = open_project(project)?;
                capture(project, options, args.dry_run, |project, options| {
                    config::add_field(
                        project,
                        &field_key(&args.key)?,
                        field_type(&args.ty)?,
                        args.required,
                        args.default.as_deref(),
                        options,
                    )
                })
                .and_then(|outcome| render_outcome(&outcome, json, "add field"))
            }
        },
        ConfigCommand::Remove(args) => match args.command {
            RemoveCommand::Category(args) => {
                let project = open_project(project)?;
                capture(project, options, args.dry_run, |project, options| {
                    config::remove_category(project, &category_key(&args.key)?, options)
                })
                .and_then(|outcome| render_outcome(&outcome, json, "remove category"))
            }
        },
    }
}

/// Run one write, honouring `--dry-run`.
fn capture(
    project: Project,
    options: &OpOptions,
    dry_run: bool,
    body: impl FnOnce(&Project, &OpOptions) -> Result<ConfigOutcome, WaymarkError>,
) -> Result<ConfigOutcome, WaymarkError> {
    let options = if dry_run {
        options.clone().dry_run()
    } else {
        options.clone()
    };
    body(&project, &options)
}

fn category_key(literal: &str) -> Result<CategoryKey, WaymarkError> {
    CategoryKey::parse(literal).map_err(|error| argument("key", error.to_string()))
}

fn field_key(literal: &str) -> Result<FieldKey, WaymarkError> {
    FieldKey::parse(literal).map_err(|error| argument("key", error.to_string()))
}

fn field_type(literal: &str) -> Result<FieldType, WaymarkError> {
    FieldType::parse(literal).map_err(|reason| argument("type", reason))
}

fn argument(name: &str, reason: String) -> WaymarkError {
    WaymarkError::validation(name, vec![waymark_core::ValidationIssue::new(name, reason)])
}

/// The machine form of one configuration document.
pub fn config_json(config: &waymark_core::Config) -> Value {
    serde_json::json!({
        "version": config.version.to_string(),
        "generation": config.version.generation,
        "revision": config.version.revision,
        "storage_root": config.storage_root,
        "operator": config.operator,
        "timezone": config.timezone.name(),
        "categories": config
            .categories
            .iter()
            .map(|(key, category)| {
                (
                    key.as_str().to_owned(),
                    serde_json::json!({
                        "key": key.as_str(),
                        "path": category.path,
                        "name": category.name,
                    }),
                )
            })
            .collect::<serde_json::Map<String, Value>>(),
        "fields": config
            .fields
            .iter()
            .map(|(key, declaration)| {
                (
                    key.as_str().to_owned(),
                    serde_json::json!({
                        "key": key.as_str(),
                        "type": declaration.ty.as_str(),
                        "required": declaration.required,
                        "default": declaration.default,
                    }),
                )
            })
            .collect::<serde_json::Map<String, Value>>(),
        "cache": {"enabled": config.cache.enabled, "capacity": config.cache.capacity},
    })
}

fn render_config(config: &waymark_core::Config, json: bool) -> Result<(), WaymarkError> {
    if json {
        emit(&success_envelope(
            serde_json::json!({"config": config_json(config)}),
            Meta::direct_file_scoped(),
        ))?;
        return Ok(());
    }
    // The document itself on stdout, so `config show > config.toml` works.
    print!("{}", config.to_toml_string());
    Ok(())
}

fn render_findings(findings: &[Finding], json: bool) -> Result<(), WaymarkError> {
    if findings.is_empty() {
        if json {
            emit(&success_envelope(
                serde_json::json!({"valid": true, "findings": []}),
                Meta::direct_file_scoped(),
            ))?;
        } else {
            err_line("the configuration is valid");
        }
        return Ok(());
    }

    // Findings are errors when they are configuration errors: `CONFIG_INVALID`
    // has an exit class, so a validation run that finds problems fails with it
    // instead of exiting 0. The failure envelope is printed by the one shared
    // path in `main`, so this never writes a second envelope. In human mode the
    // per-key narrative is printed here, before the failure summary.
    if !json {
        err_line(&format!("{} configuration issue(s)", findings.len()));
        for finding in findings {
            err_line(&format!("  {}: {}", finding.code, finding.message));
            if let Some(Location::ConfigKey { key }) = &finding.location {
                err_line(&format!("    at {key}"));
            }
        }
    }
    Err(WaymarkError::config_invalid(
        "config.toml",
        findings
            .iter()
            .map(|finding| finding.message.clone())
            .collect(),
    ))
}

fn render_outcome(outcome: &ConfigOutcome, json: bool, action: &str) -> Result<(), WaymarkError> {
    if json {
        emit(&success_envelope(
            serde_json::json!({
                "config": config_json(&outcome.config),
                "changed": outcome.changed,
                "changed_keys": outcome.changed_keys,
                "previous_revision": outcome.previous_revision,
                "revision": outcome.revision,
            }),
            Meta::direct_file_scoped(),
        ))?;
        return Ok(());
    }
    if outcome.changed {
        err_line(&format!(
            "{action}: revision {} -> {}",
            outcome.previous_revision, outcome.revision
        ));
        for key in &outcome.changed_keys {
            err_line(&format!("  {key}"));
        }
    } else {
        err_line(&format!("{action}: nothing to change"));
    }
    Ok(())
}
