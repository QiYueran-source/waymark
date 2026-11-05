//! `waymark config show|validate|set|add|remove`: the configuration surface.
//!
//! Every arm is thin: the domain decides what is allowed and what changed, the
//! adapter decides how a person or a parser reads it. Human output is prose on
//! stderr; `--json` is one envelope on stdout. No arm re-implements a rule.

use clap::{Args, Subcommand};
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
    /// Print the shared configuration, the local document, and the effective
    /// values with their sources.
    Show,
    /// Report every problem in either configuration document.
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

    /// Local scalar name: operator, timezone, cache.enabled, cache.capacity.
    #[arg(value_name = "NAME", required = true)]
    pub name: Option<String>,

    /// New value. An empty `operator` removes the local override.
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
            render_bundle(&config::bundle(&project)?, json)
        }
        ConfigCommand::Validate => {
            // Validation must work even when the document cannot be opened as a
            // project, so the file is located without parsing it. The local
            // document is validated alongside it whenever the project opens.
            let path = match project {
                Some(dir) => config::locate_config_path(dir)?,
                None => config::locate_config_path(&invocation_dir()?)?,
            };
            let mut findings = config::validate_config_path(&path);
            if let Some(dir) = project {
                if let Ok(project) = Project::discover(dir) {
                    findings.extend(waymark_core::ops::local_config::validate_path(
                        &project.user_config_path(),
                    ));
                }
            }
            render_findings(&findings, json)
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
                let options = if args.dry_run {
                    options.clone().dry_run()
                } else {
                    options.clone()
                };
                let outcome = waymark_core::ops::local_config::set_scalar(
                    &project, scalar, &value, &options,
                )?;
                render_local_outcome(&outcome, json, &format!("set {}", scalar.name()))
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

fn render_bundle(
    bundle: &waymark_core::ops::local_config::Bundle,
    json: bool,
) -> Result<(), WaymarkError> {
    if json {
        emit(&success_envelope(
            waymark_core::view::config_bundle_json(bundle),
            Meta::direct_file_scoped(),
        ))?;
        return Ok(());
    }
    // The shared document verbatim on stdout, so `config show > config.toml`
    // keeps working; everything local is prose on stderr, because it is not part
    // of the shared baseline and must never be redirected into one.
    print!("{}", bundle.project.to_toml_string());
    render_local_human(bundle);
    Ok(())
}

/// The local half of `config show`: what the document states, what is effective,
/// and where each effective value came from.
fn render_local_human(bundle: &waymark_core::ops::local_config::Bundle) {
    let effective = &bundle.effective;
    match &bundle.local {
        Some(local) => {
            err_line(&format!(
                "local {}:",
                waymark_core::user_config::USER_CONFIG_FILE
            ));
            let document = local.to_toml_string();
            for line in document.lines() {
                err_line(&format!("  {line}"));
            }
        }
        None => err_line(&format!(
            "local {}: absent",
            waymark_core::user_config::USER_CONFIG_FILE
        )),
    }
    err_line("effective:");
    err_line(&format!(
        "  operator    {} ({})",
        effective.operator.as_deref().unwrap_or("- unset"),
        effective.operator_source
    ));
    err_line(&format!(
        "  timezone    {} ({})",
        effective.timezone.name(),
        effective.timezone_source
    ));
    err_line(&format!(
        "  cache.enabled  {} ({})",
        effective.cache.enabled, effective.cache_enabled_source
    ));
    err_line(&format!(
        "  cache.capacity  {} ({})",
        effective.cache.capacity, effective.cache_capacity_source
    ));
    for warning in &effective.warnings {
        err_line(&format!("warning: {}: {}", warning.code, warning.message));
    }
}

fn render_local_outcome(
    outcome: &waymark_core::ops::local_config::LocalOutcome,
    json: bool,
    action: &str,
) -> Result<(), WaymarkError> {
    if json {
        emit(&success_envelope(
            serde_json::json!({
                "local": waymark_core::view::local_config_json(Some(&outcome.local)),
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
            "{action}: {} (shared revision {} unchanged)",
            outcome
                .local
                .to_toml_string()
                .trim_end()
                .replace('\n', "; "),
            outcome.revision
        ));
    } else {
        err_line(&format!("{action}: nothing to change"));
    }
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
                "config": waymark_core::view::config_json(&outcome.config),
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
