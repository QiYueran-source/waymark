//! The in-process projection: a disposable, fingerprinted view of the files.
//!
//! Work-item documents are the sole authority. A [`Projection`] is derived from
//! them, lives only in memory, and is never written back. It exists so that a
//! read can answer two different questions honestly:
//!
//! - **global reads** (`list`, `tree`, readiness, `blocked`, statistics,
//!   archive, cross-file relationship writes) need a *complete, fresh* world —
//!   every document parsed, and the inventory unchanged across the scan. Those
//!   reads call [`Projection::require_complete`] and fail with
//!   `INCOMPLETE_PROJECTION` rather than publish a partial graph.
//! - **scoped local reads** (`show`) need one document and its direct
//!   relationships. [`Projection::scoped_read`] answers from the parsed part of
//!   an otherwise incomplete projection and labels itself `scoped`.
//!
//! # Completeness and freshness
//!
//! A build walks the inventory twice: once to parse (or retain) documents, once
//! to verify that every path, size, and modification time still matches. If the
//! second walk disagrees, the snapshot was torn by a concurrent writer; the
//! build retries a bounded number of times, then fails with retryable
//! `SNAPSHOT_UNSTABLE`. A verified build is [`Completeness::Complete`] when
//! every document parsed and the inventory contained no uncertain
//! item-shaped path; it is [`Completeness::Scoped`] as soon as one document
//! failed or one path that is *shaped* like a work-item document could not be
//! inventoried (a case variant or an unreadable entry). A file that is plainly
//! not a work item — a stray `README.md`, a `.txt` note — is project noise and
//! does not by itself make the world partial; a symlink is the store's own
//! refusal, not a document.
//! Freshness is [`Freshness::Fresh`] only after that verification.
//!
//! # Content fingerprints, not timestamps
//!
//! A refresh compares cheap inventory signals first. When they differ it reads
//! the bytes and compares the content fingerprint: an equal fingerprint retains
//! the parsed entry (a metadata-only change) and updates the recorded size and
//! modification time. A parse happens only when the content really changed, so
//! [`ProjectionStats::parse_count`] is a stable witness for that rule.
//!
//! # The bounded cache
//!
//! [`Projection::cache_put`] caches *derived* data only — bodies, details, and
//! query results — keyed by [`CacheKey`], which carries the content fingerprint
//! the data was derived from. Parsed items and their relationship adjacency are
//! the static projection and are never cache entries, so eviction can never
//! remove a relationship node. A cache configuration change
//! ([`Projection::reconfigure_cache`]) clears evictable data and leaves the
//! static projection untouched. No cache setting can weaken freshness,
//! completeness, ownership checks, or archive safety; those are decided before
//! an entry is looked up.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use chrono::{DateTime, Utc};

use crate::config::CacheSettings;
use crate::envelope::{Completeness, Finding, Freshness, Meta, Source};
use crate::error::{incomplete_projection_message, Code, WaymarkError};
use crate::id::WorkItemId;
use crate::location::Location;
use crate::model::{CategoryKey, WorkItem};
use crate::readiness::{Graph, Readiness};
use crate::store::{Fingerprint, InventoryEntry, InventoryProblem, Project};

/// How many times a read retries before it reports `SNAPSHOT_UNSTABLE`.
pub const DEFAULT_READ_ATTEMPTS: u32 = 3;

/// How one projection build behaves.
///
/// The defaults are what production uses. [`LoadOptions::with_inventory_hook`]
/// is a test seam: it runs before every inventory pass, which lets a test act as
/// a controlled concurrent writer without racing a real thread.
#[derive(Clone)]
pub struct LoadOptions {
    /// The bounded retry budget. A value below one is treated as one.
    pub max_attempts: u32,
    /// Called before each inventory pass with the zero-based pass ordinal.
    pub on_inventory: Option<Arc<dyn Fn(u32) + Send + Sync>>,
}

impl LoadOptions {
    /// The production policy: three attempts, no observer.
    pub fn new() -> Self {
        Self {
            max_attempts: DEFAULT_READ_ATTEMPTS,
            on_inventory: None,
        }
    }

    /// Change the retry budget.
    pub fn with_max_attempts(mut self, max_attempts: u32) -> Self {
        self.max_attempts = max_attempts;
        self
    }

    /// Install a test hook run before every inventory pass.
    pub fn with_inventory_hook<F>(mut self, hook: F) -> Self
    where
        F: Fn(u32) + Send + Sync + 'static,
    {
        self.on_inventory = Some(Arc::new(hook));
        self
    }
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for LoadOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadOptions")
            .field("max_attempts", &self.max_attempts)
            .field("on_inventory", &self.on_inventory.is_some())
            .finish()
    }
}

/// One parsed document in the projection.
#[derive(Debug, Clone)]
pub struct ProjectedEntry {
    /// The document path.
    pub path: Utf8PathBuf,
    /// The stable ID, validated against the file name.
    pub id: WorkItemId,
    /// The category directory the document was found under.
    pub category: CategoryKey,
    /// The size observed for the bytes that were parsed.
    pub size: u64,
    /// The modification time observed with those bytes.
    pub modified: Option<DateTime<Utc>>,
    /// The content fingerprint of the bytes that were parsed.
    pub fingerprint: Fingerprint,
    /// The parsed document.
    pub item: WorkItem,
}

impl ProjectedEntry {
    fn with_metadata(&self, entry: &InventoryEntry) -> Self {
        let mut retained = self.clone();
        retained.size = entry.size;
        retained.modified = entry.modified;
        retained
    }
}

/// One document the projection could not turn into an item.
#[derive(Debug, Clone)]
pub struct ProjectionFailure {
    /// The document path.
    pub path: Utf8PathBuf,
    /// The fingerprint of the bytes, when they were read.
    pub fingerprint: Option<Fingerprint>,
    /// Why the document is not in the projection.
    pub error: WaymarkError,
}

/// Work a projection build performed, so tests can assert cache and parse rules
/// without timing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProjectionStats {
    /// Documents actually parsed (a metadata-only refresh does not parse).
    pub parse_count: usize,
    /// Cache lookups that found a current entry.
    pub cache_hits: usize,
    /// Cache lookups that found nothing, or found an entry for a stale
    /// fingerprint.
    pub cache_misses: usize,
    /// Cache entries dropped to respect the capacity.
    pub cache_evictions: usize,
    /// Inventory passes performed, including verification passes.
    pub inventory_passes: usize,
    /// Attempts discarded because the inventory moved underneath them.
    pub retries: usize,
}

impl ProjectionStats {
    fn merge(&mut self, other: &Self) {
        self.parse_count += other.parse_count;
        self.cache_hits += other.cache_hits;
        self.cache_misses += other.cache_misses;
        self.cache_evictions += other.cache_evictions;
        self.inventory_passes += other.inventory_passes;
        self.retries += other.retries;
    }
}

/// The kind of derived data a cache entry holds.
///
/// There is deliberately no `Item` or `Graph` variant: relationship nodes are
/// the static projection, never cache entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CacheClass {
    /// A document body.
    Body,
    /// Rendered detail data for one item.
    Detail,
    /// A query result.
    Query,
}

/// The identity of one cache entry: what it is, what it is about, and the
/// content fingerprint it was derived from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheKey {
    /// The kind of derived data.
    pub class: CacheClass,
    /// An item ID for `Body`/`Detail`, or a caller-defined query identity.
    pub subject: String,
    /// The content fingerprint the value was derived from.
    pub fingerprint: Fingerprint,
}

impl CacheKey {
    /// A key for `subject` at `fingerprint`.
    pub fn new(class: CacheClass, subject: impl Into<String>, fingerprint: &Fingerprint) -> Self {
        Self {
            class,
            subject: subject.into(),
            fingerprint: fingerprint.clone(),
        }
    }

    /// A body of `id` as parsed from `fingerprint`.
    pub fn body(id: &WorkItemId, fingerprint: &Fingerprint) -> Self {
        Self::new(CacheClass::Body, id.as_str(), fingerprint)
    }

    /// Detail data for `id` as parsed from `fingerprint`.
    pub fn detail(id: &WorkItemId, fingerprint: &Fingerprint) -> Self {
        Self::new(CacheClass::Detail, id.as_str(), fingerprint)
    }

    /// A query result, bound to the [`Projection::stamp`] it was computed from.
    pub fn query(identity: impl Into<String>, stamp: &Fingerprint) -> Self {
        Self::new(CacheClass::Query, identity, stamp)
    }
}

/// A small bounded LRU over derived values.
#[derive(Debug, Clone, Default)]
struct LruCache {
    capacity: usize,
    entries: VecDeque<(CacheKey, String)>,
}

impl LruCache {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            entries: VecDeque::new(),
        }
    }

    fn get(&mut self, key: &CacheKey) -> Option<&str> {
        let position = self.entries.iter().position(|(entry, _)| entry == key)?;
        let entry = self.entries.remove(position)?;
        self.entries.push_back(entry);
        self.entries.back().map(|(_, value)| value.as_str())
    }

    /// Insert `value`, returning how many entries were evicted.
    fn put(&mut self, key: CacheKey, value: String) -> usize {
        if self.capacity == 0 {
            return 0;
        }
        let mut evicted = 0;
        if let Some(position) = self
            .entries
            .iter()
            .position(|(existing, _)| existing == &key)
        {
            self.entries.remove(position);
        }
        while self.entries.len() >= self.capacity {
            self.entries.pop_front();
            evicted += 1;
        }
        self.entries.push_back((key, value));
        evicted
    }

    fn retain(&mut self, mut keep: impl FnMut(&CacheKey) -> bool) {
        self.entries.retain(|(key, _)| keep(key));
    }

    fn clear(&mut self) {
        self.entries.clear();
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

/// The result of a scoped local read: one item plus its direct relationships.
#[derive(Debug, Clone)]
pub struct ScopedRead {
    /// The document.
    pub item: WorkItem,
    /// The document path.
    pub path: Utf8PathBuf,
    /// The category directory that holds the document.
    pub category: CategoryKey,
    /// The content fingerprint of the bytes that were read.
    pub fingerprint: Fingerprint,
    /// Derived readiness, or `None` when the projection is incomplete: a broken
    /// sibling could hide the dependency that decides it.
    pub readiness: Option<Readiness>,
    /// The active items whose parent is this item.
    pub children: Vec<WorkItemId>,
    /// The active items that depend on this item.
    pub dependents: Vec<WorkItemId>,
    /// The `_meta` block: `scoped`, fresh, with the diagnostics this read knows.
    pub meta: Meta,
}

/// One parsed or failed document, before it is assembled into a projection.
#[derive(Debug, Default)]
struct Scan {
    entries: BTreeMap<Utf8PathBuf, ProjectedEntry>,
    failures: BTreeMap<Utf8PathBuf, ProjectionFailure>,
    problems: Vec<InventoryProblem>,
    observations: BTreeMap<Utf8PathBuf, Observation>,
}

#[derive(Debug, Clone, Copy)]
struct Observation {
    size: u64,
    modified: Option<DateTime<Utc>>,
}

/// The in-process, disposable projection of one project.
#[derive(Debug)]
pub struct Projection {
    entries: BTreeMap<Utf8PathBuf, ProjectedEntry>,
    failures: BTreeMap<Utf8PathBuf, ProjectionFailure>,
    problems: Vec<InventoryProblem>,
    completeness: Completeness,
    freshness: Freshness,
    cache: LruCache,
    cache_settings: CacheSettings,
    stats: ProjectionStats,
}

impl Projection {
    /// Build a projection from the files.
    pub fn load(project: &Project) -> Result<Self, WaymarkError> {
        Self::load_with(project, LoadOptions::new())
    }

    /// Build a projection with explicit retry and observer options.
    pub fn load_with(project: &Project, options: LoadOptions) -> Result<Self, WaymarkError> {
        let (scan, stats) = read_verified(project, None, &options)?;
        Ok(Self::assemble(project, scan, stats))
    }

    fn assemble(project: &Project, scan: Scan, stats: ProjectionStats) -> Self {
        let cache_settings = project.config().cache;
        Self {
            completeness: completeness_of(&scan.failures, &scan.problems),
            entries: scan.entries,
            failures: scan.failures,
            problems: scan.problems,
            freshness: Freshness::Fresh,
            cache: LruCache::with_capacity(if cache_settings.enabled {
                cache_settings.capacity
            } else {
                0
            }),
            cache_settings,
            stats,
        }
    }

    /// Rebuild from the files, retaining unchanged entries and evictable data.
    ///
    /// Content fingerprints, never timestamps, decide whether an entry is
    /// re-parsed.
    pub fn refresh(&mut self, project: &Project) -> Result<(), WaymarkError> {
        self.refresh_with(project, LoadOptions::new())
    }

    /// Refresh with explicit retry and observer options.
    pub fn refresh_with(
        &mut self,
        project: &Project,
        options: LoadOptions,
    ) -> Result<(), WaymarkError> {
        let (scan, stats) = read_verified(project, Some(self), &options)?;
        self.entries = scan.entries;
        self.failures = scan.failures;
        self.problems = scan.problems;
        self.completeness = completeness_of(&self.failures, &self.problems);
        self.freshness = Freshness::Fresh;
        self.stats.merge(&stats);

        let settings = project.config().cache;
        if settings == self.cache_settings {
            self.prune_cache();
        } else {
            self.reconfigure_cache(settings);
        }
        Ok(())
    }

    /// Whether every document in the project is in the projection.
    pub fn completeness(&self) -> Completeness {
        self.completeness
    }

    /// Whether the projection was verified against the files it was built from.
    pub fn freshness(&self) -> Freshness {
        self.freshness
    }

    /// Whether the projection is complete and fresh.
    pub fn is_complete(&self) -> bool {
        self.completeness == Completeness::Complete && self.freshness == Freshness::Fresh
    }

    /// Every parsed entry, in path order.
    pub fn entries(&self) -> impl Iterator<Item = &ProjectedEntry> {
        self.entries.values()
    }

    /// Every parsed document, in path order.
    pub fn items(&self) -> impl Iterator<Item = &WorkItem> {
        self.entries.values().map(|entry| &entry.item)
    }

    /// The parsed entry for `id`, when the projection holds one.
    pub fn entry(&self, id: &WorkItemId) -> Option<&ProjectedEntry> {
        self.entries.values().find(|entry| &entry.id == id)
    }

    /// The content fingerprint of `id`, when the projection holds it.
    pub fn fingerprint_of(&self, id: &WorkItemId) -> Option<&Fingerprint> {
        self.entry(id).map(|entry| &entry.fingerprint)
    }

    /// Every document that could not be parsed, in path order.
    pub fn failures(&self) -> impl Iterator<Item = &ProjectionFailure> {
        self.failures.values()
    }

    /// Files the inventory walk skipped, and why.
    ///
    /// A skipped path is not a parse failure: it never entered the projection at
    /// all. A diagnostic run reports it so an uncertain inventory is never
    /// mistaken for a complete one.
    pub fn problems(&self) -> &[InventoryProblem] {
        &self.problems
    }

    /// Documents that could not be read: `(unreadable_documents,
    /// uncertain_paths)`.
    ///
    /// `unreadable_documents` counts the parse/read failures; `uncertain_paths`
    /// counts the inventory paths whose item shape could not be classified. The
    /// pair is what [`Code::IncompleteProjection`](crate::error::Code::IncompleteProjection)
    /// reports as the concrete shortfall, and [`Projection::diagnostics`] uses
    /// the same counts so the prose can never disagree with the findings.
    pub fn shortfall_counts(&self) -> (usize, usize) {
        (
            self.unreadable_document_count(),
            self.uncertain_document_path_count(),
        )
    }

    /// How many documents could not be read.
    pub fn unreadable_document_count(&self) -> usize {
        self.failures.len()
    }

    /// How many item-shaped inventory paths are uncertain.
    ///
    /// A path that is not item-shaped, or that is a symlink (which the store
    /// deliberately never follows), is not counted: it cannot hide a document.
    pub fn uncertain_document_path_count(&self) -> usize {
        self.problems
            .iter()
            .filter(|problem| concerns_a_document(&problem.path))
            .count()
    }

    /// The paths that could not be parsed or read, or that the inventory could
    /// not classify as a document.
    ///
    /// A case-variant path is included even though it never became a
    /// [`ProjectionFailure`]: it is exactly the uncertainty that makes the
    /// projection scoped, and a caller reporting what is missing should name it.
    pub fn incomplete_paths(&self) -> Vec<&Utf8Path> {
        let mut paths: Vec<&Utf8Path> = self
            .failures
            .keys()
            .map(Utf8PathBuf::as_path)
            .chain(
                self.problems
                    .iter()
                    .filter(|problem| concerns_a_document(&problem.path))
                    .map(|problem| problem.path.as_path()),
            )
            .collect();
        paths.sort_unstable();
        paths
    }

    /// The findings that explain an incomplete projection.
    ///
    /// A complete projection has no findings here: dangling references and
    /// cycles are graph diagnostics, not projection diagnostics.
    pub fn diagnostics(&self) -> Vec<Finding> {
        let mut findings = Vec::new();
        let uncertain: Vec<&InventoryProblem> = self
            .problems
            .iter()
            .filter(|problem| concerns_a_document(&problem.path))
            .collect();
        if self.completeness == Completeness::Scoped {
            let shortfall = self.shortfall_counts();
            findings.push(Finding::new(
                Code::IncompleteProjection,
                incomplete_projection_message(
                    self.completeness.as_str(),
                    self.freshness.as_str(),
                    Some(shortfall),
                ),
            ));
        }
        for failure in self.failures.values() {
            let mut finding = Finding::new(failure.error.code(), failure.error.message());
            if let Some(location) = failure.error.location() {
                finding = finding.with_location(location.clone());
            }
            findings.push(finding);
        }
        for problem in uncertain {
            findings.push(inventory_finding(problem, self));
        }
        findings
    }

    /// The `_meta` a whole-project read justifies.
    pub fn meta(&self) -> Meta {
        Meta {
            source: Source::MemoryProjection,
            completeness: self.completeness,
            freshness: self.freshness,
            diagnostics: self.diagnostics(),
            warnings: Vec::new(),
        }
    }

    /// Fail with `INCOMPLETE_PROJECTION` unless the projection is complete and
    /// fresh.
    pub fn require_complete(&self) -> Result<(), WaymarkError> {
        if self.is_complete() {
            return Ok(());
        }
        let diagnostics: Vec<serde_json::Value> = self
            .diagnostics()
            .iter()
            .filter_map(|finding| serde_json::to_value(finding).ok())
            .collect();
        let (unreadable_documents, uncertain_paths) = self.shortfall_counts();
        Err(WaymarkError::incomplete_projection_shortfall(
            self.completeness.as_str(),
            self.freshness.as_str(),
            unreadable_documents,
            uncertain_paths,
            diagnostics,
        ))
    }

    /// Build a graph from whatever the projection parsed.
    ///
    /// On an incomplete projection this graph is partial: callers that need the
    /// whole project must call [`Projection::require_complete`] first.
    pub fn graph(&self) -> Graph {
        let items: Vec<WorkItem> = self.items().cloned().collect();
        Graph::from_items(&items)
    }

    /// Read one item plus its direct relationships, without requiring the whole
    /// project to parse.
    ///
    /// The result is always labelled `scoped`; derived readiness is omitted when
    /// the projection is incomplete.
    pub fn scoped_read(&self, id: &WorkItemId) -> Result<ScopedRead, WaymarkError> {
        let Some(entry) = self.entry(id) else {
            if let Some(failure) = self.failure_for(id) {
                return Err(failure.error.clone());
            }
            return Err(WaymarkError::item_not_found(id));
        };
        let graph = self.graph();
        let complete = self.is_complete();
        let mut meta = Meta::direct_file_scoped();
        meta.diagnostics = if complete {
            graph.diagnostics()
        } else {
            self.diagnostics()
        };
        Ok(ScopedRead {
            item: entry.item.clone(),
            path: entry.path.clone(),
            category: entry.category.clone(),
            fingerprint: entry.fingerprint.clone(),
            readiness: if complete {
                graph.readiness_any(id)
            } else {
                None
            },
            children: graph.children(id),
            dependents: graph.dependents(id),
            meta,
        })
    }

    /// A digest of every content fingerprint and failure in the projection.
    ///
    /// A metadata-only change leaves it unchanged; any content change moves it.
    /// Query cache entries are bound to this value.
    pub fn stamp(&self) -> Fingerprint {
        let mut bytes = Vec::new();
        for (path, entry) in &self.entries {
            bytes.extend_from_slice(path.as_str().as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(entry.fingerprint.as_str().as_bytes());
            bytes.push(0);
        }
        for path in self.failures.keys() {
            bytes.extend_from_slice(b"!");
            bytes.extend_from_slice(path.as_str().as_bytes());
            bytes.push(0);
        }
        Fingerprint::of(&bytes)
    }

    /// The work this projection has performed.
    pub fn stats(&self) -> &ProjectionStats {
        &self.stats
    }

    /// Look up a derived cache entry.
    ///
    /// The key's fingerprint must still be current: a `Body`/`Detail` key is
    /// checked against the parsed entry's fingerprint, and a `Query` key against
    /// [`Projection::stamp`]. A stale key is a miss.
    pub fn cache_get(&mut self, key: &CacheKey) -> Option<&str> {
        if !self.cache_settings.enabled {
            return None;
        }
        if !self.cache_key_is_current(key) {
            self.stats.cache_misses += 1;
            return None;
        }
        let value = self.cache.get(key);
        if value.is_some() {
            self.stats.cache_hits += 1;
        } else {
            self.stats.cache_misses += 1;
        }
        value
    }

    /// Store derived data in the bounded cache.
    ///
    /// A stale key is ignored: the cache never becomes a second source of truth.
    pub fn cache_put(&mut self, key: CacheKey, value: impl Into<String>) {
        if !self.cache_settings.enabled {
            return;
        }
        if !self.cache_key_is_current(&key) {
            return;
        }
        self.stats.cache_evictions += self.cache.put(key, value.into());
    }

    /// How many derived entries the cache holds.
    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    /// Whether the configured cache accepts entries.
    pub fn cache_enabled(&self) -> bool {
        self.cache_settings.enabled
    }

    /// Drop every derived entry, leaving the static projection untouched.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Apply new cache settings.
    ///
    /// A configuration change clears evictable data but never touches the parsed
    /// entries, their adjacency, or this projection's completeness and freshness.
    pub fn reconfigure_cache(&mut self, settings: CacheSettings) {
        if settings == self.cache_settings {
            return;
        }
        self.cache.clear();
        self.cache_settings = settings;
        self.cache.capacity = if settings.enabled {
            settings.capacity
        } else {
            0
        };
    }

    fn failure_for(&self, id: &WorkItemId) -> Option<&ProjectionFailure> {
        self.failures
            .values()
            .find(|failure| failure.path.file_stem() == Some(id.as_str()))
    }

    fn cache_key_is_current(&self, key: &CacheKey) -> bool {
        match key.class {
            CacheClass::Body | CacheClass::Detail => match WorkItemId::parse(&key.subject) {
                Ok(id) => self
                    .entry(&id)
                    .is_some_and(|entry| entry.fingerprint == key.fingerprint),
                Err(_) => false,
            },
            CacheClass::Query => key.fingerprint == self.stamp(),
        }
    }

    fn prune_cache(&mut self) {
        let stamp = self.stamp();
        let fingerprints: BTreeMap<String, Fingerprint> = self
            .entries
            .values()
            .map(|entry| (entry.id.as_str().to_owned(), entry.fingerprint.clone()))
            .collect();
        self.cache.retain(|key| match key.class {
            CacheClass::Query => key.fingerprint == stamp,
            CacheClass::Body | CacheClass::Detail => fingerprints
                .get(&key.subject)
                .is_some_and(|fingerprint| *fingerprint == key.fingerprint),
        });
    }
}

/// Whether an inventory path is *shaped* like a work-item document.
///
/// The inventory refuses a file whose stem is a case variant of a stable ID
/// (for example `WM-....md`) rather than adding it to [`Inventory::entries`].
/// That is not project noise: the file names the same ID space Waymark reads,
/// so a global read cannot claim to have seen the whole world.
///
/// The distinction is deliberately the one `ops::doctor` already uses: a path
/// whose stem parses as a [`WorkItemId`] concerns a document; a stray
/// `README.md`, a `.txt` note, or a malformed name like `wm-short.md` does not.
/// Those never enter the projection, and treating them as a partial world would
/// make every global read fail for a file Waymark would never read anyway.
///
/// A symbolic link is excluded even when its stem is a stable ID: the store
/// deliberately never follows one, so a link is not a document and the problem
/// is the refusal itself. That keeps a link in place of a document a local
/// [`Code::ItemNotFound`](crate::error::Code::ItemNotFound), not a global
/// incompleteness.
fn concerns_a_document(path: &Utf8Path) -> bool {
    if WorkItemId::parse(path.file_stem().unwrap_or_default()).is_err() {
        return false;
    }
    !std::fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
}

/// The completeness a verified scan justifies.
///
/// A parse/read failure always makes the projection scoped. An uncertain
/// inventory path does too, but only when it concerns an item-shaped document;
/// non-item noise stays out of the completeness decision.
fn completeness_of(
    failures: &BTreeMap<Utf8PathBuf, ProjectionFailure>,
    problems: &[InventoryProblem],
) -> Completeness {
    if failures.is_empty()
        && !problems
            .iter()
            .any(|problem| concerns_a_document(&problem.path))
    {
        Completeness::Complete
    } else {
        Completeness::Scoped
    }
}

/// The diagnostic for one uncertain, item-shaped inventory path.
///
/// A case variant of a live ID is a duplicate claim (the same code and wording
/// `ops::doctor` reports); a non-lowercase stem with no live counterpart is a
/// parse error; anything else is the uncertainty itself. The message always
/// names the path.
fn inventory_finding(problem: &InventoryProblem, projection: &Projection) -> Finding {
    let stem = problem.path.file_stem().unwrap_or_default();
    if let Ok(id) = WorkItemId::parse(stem) {
        if id.as_str() != stem {
            if let Some(entry) = projection.entry(&id) {
                return Finding::new(
                    Code::DuplicateId,
                    format!(
                        "the case variant {} claims the same ID as {}",
                        problem.path, entry.path
                    ),
                )
                .with_location(Location::file(problem.path.to_string()));
            }
            return Finding::new(
                Code::ParseError,
                format!(
                    "the document path {} is not the lowercase form of its stable ID",
                    problem.path
                ),
            )
            .with_location(Location::file(problem.path.to_string()));
        }
    }
    Finding::new(
        Code::IncompleteProjection,
        format!(
            "the inventory is uncertain about {}: {}",
            problem.path, problem.reason
        ),
    )
    .with_location(Location::file(problem.path.to_string()))
}

/// Build a projection, retrying while the inventory moves.
fn read_verified(
    project: &Project,
    previous: Option<&Projection>,
    options: &LoadOptions,
) -> Result<(Scan, ProjectionStats), WaymarkError> {
    let attempts = options.max_attempts.max(1);
    let mut stats = ProjectionStats::default();
    let mut pass = 0u32;
    for _ in 0..attempts {
        let scan = scan_once(project, previous, &mut pass, options, &mut stats)?;
        if verify_inventory(project, &scan, &mut pass, options, &mut stats)? {
            return Ok((scan, stats));
        }
        stats.retries += 1;
    }
    Err(WaymarkError::snapshot_unstable(attempts))
}

/// One inventory pass plus the parse (or retention) it justifies.
fn scan_once(
    project: &Project,
    previous: Option<&Projection>,
    pass: &mut u32,
    options: &LoadOptions,
    stats: &mut ProjectionStats,
) -> Result<Scan, WaymarkError> {
    run_inventory_hook(options, pass);
    let inventory = project.inventory()?;
    stats.inventory_passes += 1;

    let mut scan = Scan {
        problems: inventory.problems.clone(),
        ..Scan::default()
    };

    for entry in &inventory.entries {
        let observation = Observation {
            size: entry.size,
            modified: entry.modified,
        };
        scan.observations.insert(entry.path.clone(), observation);

        let prior = previous.and_then(|projection| projection.entries.get(&entry.path));
        if let Some(prior) = prior {
            // Cheap signals agree: the bytes cannot have changed under this
            // policy, so no read is needed.
            if prior.size == entry.size && prior.modified == entry.modified {
                scan.entries
                    .insert(entry.path.clone(), prior.with_metadata(entry));
                continue;
            }
        }

        let bytes = match project.read_bytes(&entry.path) {
            Ok(bytes) => bytes,
            Err(error) => {
                scan.failures.insert(
                    entry.path.clone(),
                    ProjectionFailure {
                        path: entry.path.clone(),
                        fingerprint: prior.map(|prior| prior.fingerprint.clone()),
                        error,
                    },
                );
                continue;
            }
        };
        let fingerprint = Fingerprint::of(&bytes);
        if let Some(prior) = prior {
            if prior.fingerprint == fingerprint {
                // Metadata-only change: the content fingerprint wins.
                scan.entries
                    .insert(entry.path.clone(), prior.with_metadata(entry));
                continue;
            }
        }

        stats.parse_count += 1;
        match project_entry(entry, &bytes, fingerprint.clone()) {
            Ok(projected) => {
                scan.entries.insert(entry.path.clone(), projected);
            }
            Err(error) => {
                scan.failures.insert(
                    entry.path.clone(),
                    ProjectionFailure {
                        path: entry.path.clone(),
                        fingerprint: Some(fingerprint),
                        // The parser names the field but cannot know the
                        // document's place in the inventory; record it here so
                        // a consumer of the failure gets both.
                        error: error.in_document(entry.path.as_str()),
                    },
                );
            }
        }
    }

    scan.reject_duplicate_ids();
    Ok(scan)
}

/// Parse `bytes` as the document for `entry`, checking the ID/path agreement.
fn project_entry(
    entry: &InventoryEntry,
    bytes: &[u8],
    fingerprint: Fingerprint,
) -> Result<ProjectedEntry, WaymarkError> {
    let item = crate::parse::parse_document(bytes, &entry.path)?;
    let stem = entry.path.file_stem().unwrap_or_default();
    if item.id.as_str() != stem {
        return Err(WaymarkError::id_path_mismatch(&item.id, &entry.path, stem));
    }
    Ok(ProjectedEntry {
        path: entry.path.clone(),
        id: entry.id.clone(),
        category: entry.category.clone(),
        size: entry.size,
        modified: entry.modified,
        fingerprint,
        item,
    })
}

impl Scan {
    /// Two documents that claim one stable ID are not a projection: neither is
    /// silently preferred.
    fn reject_duplicate_ids(&mut self) {
        let mut claims: BTreeMap<WorkItemId, Vec<Utf8PathBuf>> = BTreeMap::new();
        for entry in self.entries.values() {
            claims
                .entry(entry.id.clone())
                .or_default()
                .push(entry.path.clone());
        }
        for (id, paths) in claims {
            if paths.len() < 2 {
                continue;
            }
            let rendered: Vec<String> = paths.iter().map(|path| path.to_string()).collect();
            for path in paths {
                self.entries.remove(&path);
                self.failures.insert(
                    path.clone(),
                    ProjectionFailure {
                        path,
                        fingerprint: None,
                        error: WaymarkError::duplicate_id(&id, rendered.clone()),
                    },
                );
            }
        }
    }
}

/// A second inventory pass, compared against what the first pass observed.
fn verify_inventory(
    project: &Project,
    scan: &Scan,
    pass: &mut u32,
    options: &LoadOptions,
    stats: &mut ProjectionStats,
) -> Result<bool, WaymarkError> {
    run_inventory_hook(options, pass);
    let inventory = project.inventory()?;
    stats.inventory_passes += 1;

    if inventory.entries.len() != scan.observations.len() {
        return Ok(false);
    }
    for entry in &inventory.entries {
        match scan.observations.get(&entry.path) {
            Some(observation)
                if observation.size == entry.size && observation.modified == entry.modified => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

fn run_inventory_hook(options: &LoadOptions, pass: &mut u32) {
    if let Some(hook) = &options.on_inventory {
        hook(*pass);
    }
    *pass += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(subject: &str, fingerprint: &str) -> CacheKey {
        CacheKey::new(
            CacheClass::Body,
            subject,
            &Fingerprint::of(fingerprint.as_bytes()),
        )
    }

    #[test]
    fn the_lru_evicts_the_least_recently_used_entry() {
        let mut cache = LruCache::with_capacity(2);
        assert_eq!(cache.put(key("a", "1"), "a".to_owned()), 0);
        assert_eq!(cache.put(key("b", "1"), "b".to_owned()), 0);
        // Touch `a` so `b` becomes the oldest.
        assert_eq!(cache.get(&key("a", "1")), Some("a"));
        assert_eq!(cache.put(key("c", "1"), "c".to_owned()), 1);
        assert_eq!(cache.len(), 2);
        assert!(cache.get(&key("b", "1")).is_none());
        assert!(cache.get(&key("a", "1")).is_some());
    }
}
