//! Project-wide diagnostics: the one read-only run behind `doctor` and
//! `validate`.
//!
//! A diagnostic run answers "is this project healthy?" without changing it. It
//! reports findings from the shared error registry — severity, code, an optional
//! location, a message, and a fixability flag — and never repairs business data:
//! it never deletes a relationship, rewrites a body or custom field, restores an
//! item, or infers what an operator meant. The only thing it rebuilds is the
//! disposable in-memory projection (and its in-process cache), and it reports
//! that rebuild in [`Report::repairs`] with what changed and what remains.
//!
//! # Coverage
//!
//! A run reviews the configuration, the lock, the projection, the graph, the
//! inventory, and every document:
//!
//! - configuration findings, including path conflicts between category
//!   directories (`CONFIG_INVALID`);
//! - a non-blocking lock probe (`LOCK_HELD`, info, no wait, no holder identity);
//! - Git availability, as one finding per missing cause
//!   (`NOT_A_GIT_REPOSITORY` for the repository, `GIT_UNAVAILABLE` for the
//!   executable). The probe only reads the filesystem and `PATH`: it never
//!   installs Git and never initializes a repository;
//! - malformed and oversized frontmatter, unreadable documents, duplicate IDs,
//!   case variants, ID/path disagreements, and unsupported generations, all from
//!   the projection's failures (`PARSE_ERROR`, `VALIDATION_ERROR`,
//!   `DUPLICATE_ID`, `ID_PATH_MISMATCH`, `UNSUPPORTED_VERSION`);
//! - malformed titles — empty or multi-line — which parse fine but violate the
//!   title rule (`VALIDATION_ERROR`);
//! - undeclared or mistyped custom fields and missing required fields after a
//!   configuration change (`VALIDATION_ERROR`);
//! - missing and archived relationship targets (`DANGLING_REFERENCE`) and both
//!   cycle kinds (`CYCLE_DETECTED`), but only when the projection is complete:
//!   a partial graph is never published as a complete review;
//! - timestamp incoherence (`TIMESTAMP_INCOHERENT`, warning);
//! - items needing a format migration (`MIGRATION_AVAILABLE`, info);
//! - projection state: incompleteness, uncertain inventory entries, content
//!   fingerprint changes, and — when a caller retains a projection and runs
//!   [`diagnose_against`] — new, deleted, and moved files (`INCOMPLETE_PROJECTION`).
//!
//! # Strict mode
//!
//! A report succeeds by default whatever it contains. [`DoctorOptions::strict`]
//! makes [`Report::exit_class`] return the registry's invalid-data class when
//! error-severity findings remain; warnings and info findings never change an
//! exit code.

use std::collections::BTreeSet;

use camino::Utf8Path;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::Config;
use crate::envelope::{Completeness, Finding, Meta, Source};
use crate::error::{Code, ExitClass, Severity, WaymarkError};
use crate::git::GitProbe;
use crate::id::WorkItemId;
use crate::location::Location;
use crate::model::{FieldKey, Status, WorkItem, FORMAT_GENERATION};
use crate::ops::config::validate_config;
use crate::projection::Projection;
use crate::store::{Fingerprint, Project};

/// The parts of the project a full run reviews.
pub const SCOPE: &[&str] = &[
    "configuration",
    "work_items",
    "graph",
    "projection",
    "inventory",
    "lock",
    "git",
];

/// The scope of a configuration-only report.
pub const CONFIG_SCOPE: &[&str] = &["configuration"];

/// The largest frontmatter block a document may carry.
///
/// The specification requires oversized frontmatter to be diagnosed but does not
/// pin a number. 64 KiB is comfortably above any real document and far below a
/// size that makes the YAML reader a denial-of-service surface; the provisional
/// choice is recorded in `docs/implementation-decisions.md`.
pub const MAX_FRONTMATTER_BYTES: usize = 64 * 1024;

/// Findings counted by severity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeverityCounts {
    /// Error-severity findings.
    pub errors: usize,
    /// Warning-severity findings.
    pub warnings: usize,
    /// Info-severity findings.
    pub info: usize,
}

impl SeverityCounts {
    /// Count the findings by registry severity.
    pub fn of(findings: &[Finding]) -> Self {
        let mut counts = Self::default();
        for finding in findings {
            match finding.severity {
                Severity::Error => counts.errors += 1,
                Severity::Warning => counts.warnings += 1,
                Severity::Info => counts.info += 1,
            }
        }
        counts
    }

    /// The total number of findings.
    pub fn total(self) -> usize {
        self.errors + self.warnings + self.info
    }
}

/// The only internal state a diagnostic run may rebuild.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairKind {
    /// The disposable in-memory projection was rebuilt from the files.
    ProjectionRebuild,
    /// The in-process cache of derived data was reset.
    CacheRebuild,
}

/// One repair the run performed, and what it left for a human.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repair {
    /// What kind of internal state was rebuilt.
    pub kind: RepairKind,
    /// What changed, in human-readable lines.
    pub changed: Vec<String>,
    /// Findings that remain after the repair, as `CODE: message` lines.
    pub remaining: Vec<String>,
}

/// Everything one diagnostic run reports.
#[derive(Debug, Clone)]
pub struct Report {
    /// Every finding, in the order the run discovered them.
    pub findings: Vec<Finding>,
    /// Findings counted by severity.
    pub counts: SeverityCounts,
    /// Whether strict mode was requested.
    pub strict: bool,
    /// The parts of the project this report reviewed.
    pub scope: Vec<String>,
    /// Internal state the run rebuilt.
    pub repairs: Vec<Repair>,
    /// The envelope metadata for this report.
    pub meta: Meta,
}

impl Report {
    /// Build a report from its parts, counting the findings.
    pub fn new(
        findings: Vec<Finding>,
        strict: bool,
        scope: &[&str],
        repairs: Vec<Repair>,
        meta: Meta,
    ) -> Self {
        let counts = SeverityCounts::of(&findings);
        Self {
            findings,
            counts,
            strict,
            scope: scope.iter().map(|part| (*part).to_owned()).collect(),
            repairs,
            meta,
        }
    }

    /// Whether the report found at least one error.
    pub fn has_errors(&self) -> bool {
        self.counts.errors > 0
    }

    /// Attach one more finding, recounting the severities.
    ///
    /// Adapters use this for a check whose inputs it owns — the embedded
    /// frontend provenance is one — so the finding travels the same report,
    /// envelope, and strict-exit path as a core finding instead of growing a
    /// second diagnosis mechanism.
    pub fn push_finding(&mut self, finding: Finding) {
        self.findings.push(finding);
        self.counts = SeverityCounts::of(&self.findings);
    }

    /// The process exit class strict mode asks for.
    ///
    /// A default report always succeeds; strict mode returns the registry's
    /// invalid-data class while error-severity findings remain.
    pub fn exit_class(&self) -> Option<ExitClass> {
        if self.strict && self.has_errors() {
            Some(ExitClass::InvalidInput)
        } else {
            None
        }
    }

    /// The machine `data` block.
    pub fn data_json(&self) -> Value {
        json!({
            "scope": self.scope,
            "strict": self.strict,
            "valid": self.counts.errors == 0,
            "healthy": self.counts.errors == 0,
            "counts": self.counts,
            "findings": self.findings,
            "repairs": self.repairs,
        })
    }
}

/// How a diagnostic run behaves.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DoctorOptions {
    /// Whether error findings change the exit class.
    pub strict: bool,
}

impl DoctorOptions {
    /// The default: report findings, exit zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the invalid-data class while error findings remain.
    pub fn strict(self) -> Self {
        self.with_strict(true)
    }

    /// Set strict mode explicitly.
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }
}

/// A project opened for diagnosis, or the report that replaces it when the
/// configuration cannot be opened at all.
#[derive(Debug)]
pub enum Diagnosis {
    /// The configuration loaded; the whole project can be reviewed.
    Project(Box<Project>),
    /// The configuration is invalid or from an unsupported generation; only a
    /// configuration-scoped report can be produced.
    Configuration(Report),
}

impl Diagnosis {
    /// The project when the configuration opened.
    pub fn project(&self) -> Option<&Project> {
        match self {
            Self::Project(project) => Some(project),
            Self::Configuration(_) => None,
        }
    }

    /// The configuration-scoped report when it did not.
    pub fn configuration_report(&self) -> Option<&Report> {
        match self {
            Self::Project(_) => None,
            Self::Configuration(report) => Some(report),
        }
    }
}

/// Locate and open the project at or above `start`, keeping a broken
/// configuration diagnosable.
///
/// This is the one derivation of the project root from the located
/// configuration document, and the one place `CONFIG_INVALID` and
/// `UNSUPPORTED_VERSION` are mapped to a configuration-scoped report instead of
/// a bare failure. Adapters call [`diagnose_from`] for the complete run.
pub fn open_for_diagnosis(
    start: &Utf8Path,
    options: DoctorOptions,
) -> Result<Diagnosis, WaymarkError> {
    let config_path = crate::ops::config::locate_config_path(start)?;
    match Config::load(&config_path) {
        Ok(_) => {
            // `.waymark/config.toml` sits one directory under the project root.
            let root = config_path
                .parent()
                .and_then(Utf8Path::parent)
                .ok_or_else(|| {
                    WaymarkError::internal(format!(
                        "the configuration path {config_path} has no project root"
                    ))
                })?;
            Ok(Diagnosis::Project(Box::new(Project::open(root)?)))
        }
        Err(error) if matches!(error.code(), Code::ConfigInvalid | Code::UnsupportedVersion) => Ok(
            Diagnosis::Configuration(diagnose_config(&config_path, options)?),
        ),
        Err(error) => Err(error),
    }
}

/// Run every project-wide diagnostic starting from `start`, even when the
/// configuration at the discovered project cannot be opened.
pub fn diagnose_from(start: &Utf8Path, options: DoctorOptions) -> Result<Report, WaymarkError> {
    match open_for_diagnosis(start, options)? {
        Diagnosis::Project(project) => diagnose_with(&project, options),
        Diagnosis::Configuration(report) => Ok(report),
    }
}

/// Run every project-wide diagnostic against a project.
pub fn diagnose(project: &Project) -> Result<Report, WaymarkError> {
    diagnose_with(project, DoctorOptions::new())
}

/// Run every project-wide diagnostic with explicit options.
pub fn diagnose_with(project: &Project, options: DoctorOptions) -> Result<Report, WaymarkError> {
    run(project, None, options)
}

/// Run every diagnostic and compare the fresh projection with a retained one.
///
/// This is the seam a long-lived process (the MCP server) uses: it keeps the
/// projection it has been answering from and asks what changed underneath it.
/// New, deleted, moved, and fingerprint-changed files are reported as
/// `INCOMPLETE_PROJECTION` findings so stale data is never passed off as fresh.
pub fn diagnose_against(
    project: &Project,
    previous: &Projection,
    options: DoctorOptions,
) -> Result<Report, WaymarkError> {
    run(project, Some(previous), options)
}

/// A configuration-only report for a project whose configuration cannot be
/// opened as a [`Project`](crate::Project) at all.
///
/// `Project::open` validates the configuration, so a diagnostic command could
/// never reach the work items of a project with a broken configuration. This
/// entry point still reports every configuration finding — including an
/// unsupported generation — with a `configuration` scope, and says so rather
/// than pretending the whole project was reviewed.
pub fn diagnose_config(
    config_path: &Utf8Path,
    options: DoctorOptions,
) -> Result<Report, WaymarkError> {
    let findings = match Config::issues_in_file(config_path) {
        Ok(issues) => issues
            .iter()
            .map(|issue| {
                Finding::new(Code::ConfigInvalid, issue.render()).with_location(issue.location())
            })
            .collect(),
        Err(error) => findings_from_error(&error),
    };
    Ok(Report::new(
        findings,
        options.strict,
        CONFIG_SCOPE,
        Vec::new(),
        Meta::direct_file_scoped(),
    ))
}

fn run(
    project: &Project,
    previous: Option<&Projection>,
    options: DoctorOptions,
) -> Result<Report, WaymarkError> {
    let mut findings: Vec<Finding> = Vec::new();

    // Configuration. `Project::open` already validated it, but a diagnostic run
    // repeats the review because a hand edit can land between discovery and the
    // scan.
    findings.extend(validate_config(project));

    // The lock probe is deliberately non-blocking: `is_store_locked` opens the
    // lock file and tries the write lock once. It never waits and never learns
    // who holds it.
    // A probe never creates the lock file: if it does not exist, no process can
    // be holding it, and a diagnostic run must not add a file to the project it
    // is only supposed to read.
    let lock_path = project.lock_path();
    if lock_path.exists() && crate::lock::is_store_locked(project.root())? {
        findings.push(Finding::from_error(&WaymarkError::lock_held(
            lock_path.to_string(),
        )));
    }

    // Git is optional. The probe walks the filesystem and `PATH` and reports
    // one informational finding per missing cause; a diagnostic run must never
    // install Git or initialize a repository to "repair" the environment.
    findings.extend(GitProbe::detect(project).findings());

    let projection = Projection::load(project)?;

    findings.extend(projection_findings(&projection));
    findings.extend(inventory_problem_findings(&projection));

    let complete = projection.is_complete() && projection.problems().is_empty();
    if complete {
        let graph = projection.graph();
        findings.extend(graph.diagnostics());
        findings.extend(archived_target_findings(&projection, &graph));
    }

    findings.extend(oversized_and_drift_findings(project, &projection));

    for entry in projection.entries() {
        findings.extend(title_findings(&entry.item));
        findings.extend(field_findings(project.config(), &entry.item));
        findings.extend(timestamp_findings(&entry.item));
        if entry.item.needs_migration() {
            findings.push(
                Finding::from_error(&WaymarkError::migration_available(std::slice::from_ref(
                    &entry.id,
                )))
                .with_location(Location::work_item(&entry.id)),
            );
        }
    }

    if project.config().version.generation < FORMAT_GENERATION {
        findings.push(Finding::new(
            Code::MigrationAvailable,
            format!(
                "the configuration declares generation {}; generation {FORMAT_GENERATION} is available",
                project.config().version.generation
            ),
        ));
    }

    if let Some(previous) = previous {
        findings.extend(inventory_delta_findings(&projection, previous));
    }

    let meta = Meta {
        source: Source::MemoryProjection,
        completeness: if complete {
            projection.completeness()
        } else {
            Completeness::Scoped
        },
        freshness: projection.freshness(),
        diagnostics: projection.diagnostics(),
        warnings: Vec::new(),
    };
    let repairs = repairs(project, &projection, &findings);
    Ok(Report::new(findings, options.strict, SCOPE, repairs, meta))
}

/// Findings the projection itself justifies: incompleteness and its failures.
fn projection_findings(projection: &Projection) -> Vec<Finding> {
    let mut findings = Vec::new();
    if !projection.is_complete() {
        findings.push(Finding::new(
            Code::IncompleteProjection,
            format!(
                "the projection is {} and {}: {} document(s) could not be read",
                projection.completeness().as_str(),
                projection.freshness().as_str(),
                projection.failures().count()
            ),
        ));
        for failure in projection.failures() {
            findings.extend(findings_from_error(&failure.error));
        }
    }
    findings
}

/// Findings from the inventory walk: skipped files and case variants.
fn inventory_problem_findings(projection: &Projection) -> Vec<Finding> {
    let mut findings = Vec::new();
    for problem in projection.problems() {
        let stem = problem.path.file_stem().unwrap_or_default();
        if let Ok(id) = WorkItemId::parse(stem) {
            if id.as_str() != stem {
                if let Some(entry) = projection.entry(&id) {
                    findings.push(
                        Finding::new(
                            Code::DuplicateId,
                            format!(
                                "the case variant {} claims the same ID as {}",
                                problem.path, entry.path
                            ),
                        )
                        .with_location(Location::file(problem.path.to_string())),
                    );
                    continue;
                }
                findings.push(
                    Finding::new(
                        Code::ParseError,
                        format!(
                            "the document path {} is not the lowercase form of its stable ID",
                            problem.path
                        ),
                    )
                    .with_location(Location::file(problem.path.to_string())),
                );
                continue;
            }
        }
        findings.push(
            Finding::new(
                Code::IncompleteProjection,
                format!(
                    "the inventory is uncertain about {}: {}",
                    problem.path, problem.reason
                ),
            )
            .with_location(Location::file(problem.path.to_string())),
        );
    }
    findings
}

/// Relationship targets that exist only in the archived population.
///
/// The graph treats archived items as known so a routine read is not noisy, but
/// a health check owes the operator the distinction between "missing" and
/// "archived": both are `DANGLING_REFERENCE`.
fn archived_target_findings(
    projection: &Projection,
    graph: &crate::readiness::Graph,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for item in projection.items() {
        for target in &item.dependencies {
            if graph.is_archived(target) {
                findings.push(archived_target(&item.id, "dependencies", target));
            }
        }
        for target in &item.soft_links {
            if graph.is_archived(target) {
                findings.push(archived_target(&item.id, "soft_links", target));
            }
        }
        if let Some(target) = &item.parent {
            if graph.is_archived(target) {
                findings.push(archived_target(&item.id, "parent", target));
            }
        }
    }
    findings
}

fn archived_target(from: &WorkItemId, relation: &str, target: &WorkItemId) -> Finding {
    Finding::new(
        Code::DanglingReference,
        format!("{from} {relation} the archived item {target}"),
    )
    .with_location(Location::work_item(from))
}

/// The spec's title rule: non-empty and single-line.
///
/// A malformed title is not a read failure — the document still parses and must
/// stay readable — so a diagnostic run reports it at the `title` field instead
/// of rejecting the document. `title set` and `create` refuse the same values
/// before they ever reach a file.
fn title_findings(item: &WorkItem) -> Vec<Finding> {
    if item.title.trim().is_empty() {
        vec![validation_finding(
            &item.id,
            "title",
            "a title is required".to_owned(),
        )]
    } else if item.title.contains(['\n', '\r']) {
        vec![validation_finding(
            &item.id,
            "title",
            "a title must be a single line".to_owned(),
        )]
    } else {
        Vec::new()
    }
}

/// Undeclared, mistyped, and missing custom fields: configuration drift.
fn field_findings(config: &Config, item: &WorkItem) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (key, value) in &item.custom_fields {
        let field = format!("custom_fields.{key}");
        let Ok(parsed) = FieldKey::parse(key) else {
            findings.push(validation_finding(
                &item.id,
                &field,
                format!("`{key}` is not a valid custom-field key"),
            ));
            continue;
        };
        match config.fields.get(&parsed) {
            None => findings.push(validation_finding(
                &item.id,
                &field,
                format!("`{key}` is not declared in the configuration"),
            )),
            Some(declaration) if !declaration.ty.accepts(value) => {
                findings.push(validation_finding(
                    &item.id,
                    &field,
                    format!(
                        "expected a value of type `{}`, found {}",
                        declaration.ty.as_str(),
                        value.type_name()
                    ),
                ))
            }
            Some(_) => {}
        }
    }
    for (key, declaration) in &config.fields {
        if declaration.required && !item.custom_fields.contains_key(key.as_str()) {
            findings.push(validation_finding(
                &item.id,
                &format!("custom_fields.{key}"),
                "the declared field is required but missing".to_owned(),
            ));
        }
    }
    findings
}

fn validation_finding(id: &WorkItemId, field: &str, reason: String) -> Finding {
    Finding::new(Code::ValidationError, format!("{id}: {field}: {reason}"))
        .with_location(Location::field(field.to_owned()))
}

/// Persisted system timestamps that disagree with each other or the status.
fn timestamp_findings(item: &WorkItem) -> Vec<Finding> {
    let mut findings = Vec::new();
    if item.updated_at < item.created_at {
        findings.push(timestamp_finding(
            item,
            "updated_at",
            item.updated_at.to_string(),
            "updated_at precedes created_at",
        ));
    }
    match item.closed_at {
        Some(closed_at) => {
            if closed_at < item.created_at {
                findings.push(timestamp_finding(
                    item,
                    "closed_at",
                    closed_at.to_string(),
                    "closed_at precedes created_at",
                ));
            }
            if item.status != Status::Closed {
                findings.push(timestamp_finding(
                    item,
                    "closed_at",
                    closed_at.to_string(),
                    format!(
                        "closed_at is set while the status is {}",
                        item.status.as_str()
                    ),
                ));
            }
        }
        None if item.status == Status::Closed => findings.push(timestamp_finding(
            item,
            "closed_at",
            "absent".to_owned(),
            "the status is closed but no close timestamp is persisted",
        )),
        None => {}
    }
    if let Some(archived_at) = item.archived_at {
        if archived_at < item.created_at {
            findings.push(timestamp_finding(
                item,
                "archived_at",
                archived_at.to_string(),
                "archived_at precedes created_at",
            ));
        }
    }
    findings
}

fn timestamp_finding(
    item: &WorkItem,
    field: &str,
    value: String,
    reason: impl Into<String>,
) -> Finding {
    Finding::from_error(&WaymarkError::timestamp_incoherent(
        &item.id, field, value, reason,
    ))
}

/// Oversized frontmatter and content that moved after the projection read it.
///
/// The projection verified the inventory at build time; a diagnostic run reads
/// the bytes once more so a concurrent change is reported as stale projection
/// state instead of being silently answered from memory.
fn oversized_and_drift_findings(project: &Project, projection: &Projection) -> Vec<Finding> {
    let mut findings = Vec::new();
    for entry in projection.entries() {
        let bytes = match project.read_bytes(&entry.path) {
            Ok(bytes) => bytes,
            Err(error) => {
                findings.push(
                    Finding::new(
                        Code::IncompleteProjection,
                        format!(
                            "{} could not be re-read after the projection: {}",
                            entry.path,
                            error.message()
                        ),
                    )
                    .with_location(Location::file(entry.path.to_string())),
                );
                continue;
            }
        };
        if Fingerprint::of(&bytes) != entry.fingerprint {
            findings.push(
                Finding::new(
                    Code::IncompleteProjection,
                    format!(
                        "the fingerprint of {} changed after the projection read it",
                        entry.path
                    ),
                )
                .with_location(Location::file(entry.path.to_string())),
            );
        }
        if let Ok((frontmatter, _)) = crate::parse::split_frontmatter(&bytes, &entry.path) {
            if frontmatter.len() > MAX_FRONTMATTER_BYTES {
                findings.push(
                    Finding::new(
                        Code::ParseError,
                        format!(
                            "{} has {} bytes of frontmatter, over the {MAX_FRONTMATTER_BYTES}-byte limit",
                            entry.path,
                            frontmatter.len()
                        ),
                    )
                    .with_location(Location::file(entry.path.to_string())),
                );
            }
        }
    }
    findings
}

/// The delta between a retained projection and the freshly built one.
fn inventory_delta_findings(projection: &Projection, previous: &Projection) -> Vec<Finding> {
    let mut findings = Vec::new();
    let current_paths: BTreeSet<&Utf8Path> = projection
        .entries()
        .map(|entry| entry.path.as_path())
        .collect();

    let previous_entries: Vec<(WorkItemId, &crate::projection::ProjectedEntry)> = previous
        .entries()
        .map(|entry| (entry.id.clone(), entry))
        .collect();
    for (id, previous_entry) in previous_entries {
        match projection.entry(&id) {
            Some(current_entry) if current_entry.path != previous_entry.path => findings.push(
                Finding::new(
                    Code::IncompleteProjection,
                    format!(
                        "the retained projection is stale: {} moved from {} to {}",
                        id, previous_entry.path, current_entry.path
                    ),
                )
                .with_location(Location::work_item(&id)),
            ),
            None if !current_paths.contains(previous_entry.path.as_path()) => findings.push(
                Finding::new(
                    Code::IncompleteProjection,
                    format!(
                        "the retained projection is stale: {} was deleted ({})",
                        id, previous_entry.path
                    ),
                )
                .with_location(Location::file(previous_entry.path.to_string())),
            ),
            _ => {}
        }
    }

    for current_entry in projection.entries() {
        let previous_entry = previous
            .entries()
            .find(|entry| entry.id == current_entry.id);
        match previous_entry {
            None => findings.push(
                Finding::new(
                    Code::IncompleteProjection,
                    format!(
                        "the retained projection is stale: {} is new ({})",
                        current_entry.id, current_entry.path
                    ),
                )
                .with_location(Location::file(current_entry.path.to_string())),
            ),
            Some(previous_entry) if previous_entry.path == current_entry.path => {
                if previous_entry.fingerprint != current_entry.fingerprint {
                    findings.push(
                        Finding::new(
                            Code::IncompleteProjection,
                            format!(
                                "the retained projection is stale: the fingerprint changed for {}",
                                current_entry.path
                            ),
                        )
                        .with_location(Location::file(current_entry.path.to_string())),
                    );
                }
            }
            Some(_) => {}
        }
    }
    findings
}

/// The permitted internal repairs, with what changed and what remains.
fn repairs(project: &Project, projection: &Projection, findings: &[Finding]) -> Vec<Repair> {
    let remaining: Vec<String> = findings
        .iter()
        .filter(|finding| finding.severity == Severity::Error)
        .map(|finding| format!("{}: {}", finding.code, finding.message))
        .collect();
    let mut repairs = vec![Repair {
        kind: RepairKind::ProjectionRebuild,
        changed: vec![format!(
            "rebuilt the in-memory projection from {} document(s); {} unreadable",
            projection.entries().count(),
            projection.failures().count()
        )],
        remaining,
    }];
    if project.config().cache.enabled {
        repairs.push(Repair {
            kind: RepairKind::CacheRebuild,
            changed: vec![format!(
                "reset the in-process cache; {} derived entries remain",
                projection.cache_len()
            )],
            remaining: Vec::new(),
        });
    }
    repairs
}

/// Turn an error into findings, splitting a `VALIDATION_ERROR` into one finding
/// per offending field.
fn findings_from_error(error: &WaymarkError) -> Vec<Finding> {
    if error.code() == Code::ValidationError {
        if let Some(issues) = error.details().get("issues").and_then(Value::as_array) {
            let target = error
                .details()
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or("document");
            if !issues.is_empty() {
                return issues
                    .iter()
                    .map(|issue| {
                        let field = issue
                            .get("path")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        let reason = issue
                            .get("reason")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        Finding::new(
                            Code::ValidationError,
                            format!("{target}: {field}: {reason}"),
                        )
                        .with_location(Location::field(field.to_owned()))
                    })
                    .collect();
            }
        }
    }
    vec![Finding::from_error(error)]
}
