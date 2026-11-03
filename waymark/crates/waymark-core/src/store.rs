//! The file store: project discovery, path mapping, inventory, and the content
//! fingerprint.
//!
//! Work-item files are the sole authoritative data. This module owns *where* the
//! files are and *what bytes* they contain; it holds no domain rules about what
//! a valid item is (those live in [`crate::parse`] and [`crate::ops`]).
//!
//! # Layout
//!
//! ```text
//! <root>/
//!   .waymark/
//!     config.toml        the only configuration authority
//!     .gitignore         keeps write.lock (and later cache files) out of git
//!     write.lock         the store-wide advisory write lock
//!   work/                the configured storage root
//!     bugs/
//!       wm-3f7k9q2m4x8c.md
//! ```
//!
//! The metadata directory is found by walking up from a starting directory to
//! the nearest `.waymark/config.toml`, so a command works from anywhere inside
//! the project.

use std::fs;
use std::sync::{Arc, Mutex};

use camino::{Utf8Path, Utf8PathBuf};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::config::{Category, Config, CONFIG_FILE, METADATA_DIR};
use crate::error::WaymarkError;
use crate::id::{IdSource, RandomIds, WorkItemId, PREFIX};
use crate::model::CategoryKey;
use crate::time::{Clock, SystemClock};

/// How far `discover` walks up before giving up.
///
/// A project deeper than this is a mistake, and an unbounded walk would make a
/// mistyped command scan the whole filesystem.
pub const MAX_DISCOVERY_DEPTH: usize = 64;

/// The `.gitignore` entry the write lock needs.
pub const LOCK_IGNORE_ENTRY: &str = "write.lock";

/// How many ID candidates are tried before `ID_GENERATION_FAILED`.
pub const MAX_ID_ATTEMPTS: u32 = 64;

/// A content fingerprint: the SHA-256 of a document's bytes, lowercase hex.
///
/// The fingerprint is authoritative for conflict detection and cache
/// invalidation. Path, size, and modification time are cheap signals only.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint(String);

impl Fingerprint {
    /// The fingerprint of these bytes.
    pub fn of(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Self(hex(&hasher.finalize()))
    }

    /// The lowercase hex digest.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// One document found by an inventory walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryEntry {
    /// The file on disk.
    pub path: Utf8PathBuf,
    /// The category directory the file was found under.
    pub category: CategoryKey,
    /// The validated stable ID taken from the file stem.
    pub id: WorkItemId,
    /// Size in bytes.
    pub size: u64,
    /// Last modification time, when the filesystem reports one.
    pub modified: Option<DateTime<Utc>>,
}

/// A file the inventory walk skipped, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryProblem {
    /// The offending path.
    pub path: Utf8PathBuf,
    /// Why it was skipped.
    pub reason: String,
}

/// Everything an inventory walk found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inventory {
    /// The documents, sorted by path.
    pub entries: Vec<InventoryEntry>,
    /// Files skipped because their name is not a stable ID or their type is not
    /// a regular file.
    pub problems: Vec<InventoryProblem>,
}

impl Inventory {
    /// The IDs currently on disk.
    pub fn ids(&self) -> impl Iterator<Item = &WorkItemId> {
        self.entries.iter().map(|entry| &entry.id)
    }

    /// Whether `id` is already taken in this inventory.
    pub fn contains(&self, id: &WorkItemId) -> bool {
        self.ids().any(|known| known == id)
    }

    /// The single entry for `id`, when exactly one file claims it.
    pub fn entry(&self, id: &WorkItemId) -> Option<&InventoryEntry> {
        self.entries.iter().find(|entry| &entry.id == id)
    }

    /// The paths that claim `id`, when more than one does.
    ///
    /// A duplicate is an error the caller reports rather than resolves: the
    /// projection never chooses one file arbitrarily.
    pub fn duplicate_of(&self, id: &WorkItemId) -> Vec<&Utf8PathBuf> {
        let paths: Vec<&Utf8PathBuf> = self
            .entries
            .iter()
            .filter(|entry| &entry.id == id)
            .map(|entry| &entry.path)
            .collect();
        if paths.len() > 1 {
            paths
        } else {
            Vec::new()
        }
    }
}

/// Options for [`Project::init`].
#[derive(Debug, Clone, Default)]
pub struct InitOptions {
    /// Overwrite an existing configuration instead of reusing it.
    pub force: bool,
    /// Configuration to write. Defaults to [`Config::fresh`].
    pub config: Option<Config>,
    /// Operator identity to record.
    pub operator: Option<String>,
}

/// A handle to one Waymark project.
///
/// Cloning is cheap (the configuration is shared); a `Project` is a *view* of
/// the files, never a cache of them.
#[derive(Clone)]
pub struct Project {
    root: Utf8PathBuf,
    config: Arc<Config>,
    clock: Arc<dyn Clock>,
    ids: Arc<Mutex<Box<dyn IdSource + Send>>>,
}

impl std::fmt::Debug for Project {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Project")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl Project {
    /// Create a project rooted at `dir`.
    ///
    /// Creates the metadata directory, the configuration document, the
    /// `.waymark/.gitignore`, and every configured category directory. An
    /// existing configuration is not overwritten unless [`InitOptions::force`]
    /// is set: `init` is safe to re-run and succeeds when a project is already
    /// there.
    pub fn init(dir: &Utf8Path, opts: InitOptions) -> Result<Project, WaymarkError> {
        let root = dir.to_path_buf();
        let metadata = root.join(METADATA_DIR);
        let config_path = metadata.join(CONFIG_FILE);

        if config_path.exists() && !opts.force {
            return Self::open(&root);
        }

        fs::create_dir_all(&metadata)
            .map_err(|source| WaymarkError::io(metadata.to_string(), "create", &source))?;

        let mut config = opts.config.unwrap_or_else(Config::fresh);
        if let Some(operator) = opts.operator {
            config.operator = Some(operator);
        }
        config.validate()?;

        crate::write::replace_unchecked(&config_path, config.to_toml_string().as_bytes())?;
        ensure_gitignore(&metadata, &[LOCK_IGNORE_ENTRY])?;

        for category in config.categories.values() {
            let path = root.join(&category.path);
            fs::create_dir_all(&path)
                .map_err(|source| WaymarkError::io(path.to_string(), "create", &source))?;
        }

        Self::open(&root)
    }

    /// Open an existing project at `root` (the directory containing
    /// `.waymark/`).
    pub fn open(root: &Utf8Path) -> Result<Project, WaymarkError> {
        let root = root.to_path_buf();
        let config_path = root.join(METADATA_DIR).join(CONFIG_FILE);
        if !config_path.exists() {
            return Err(WaymarkError::project_not_found(root.to_string()));
        }
        let config = Config::load(&config_path)?;
        Ok(Project {
            root,
            config: Arc::new(config),
            clock: Arc::new(SystemClock),
            ids: Arc::new(Mutex::new(Box::new(RandomIds))),
        })
    }

    /// Find the nearest project at or above `start`.
    ///
    /// Walks up to [`MAX_DISCOVERY_DEPTH`] ancestors looking for
    /// `.waymark/config.toml`. A missing project is `PROJECT_NOT_FOUND` naming
    /// the directory the search started at.
    pub fn discover(start: &Utf8Path) -> Result<Project, WaymarkError> {
        let start = absolute(start);
        let mut current: Option<&Utf8Path> = Some(start.as_path());
        for _ in 0..MAX_DISCOVERY_DEPTH {
            let Some(directory) = current else { break };
            if directory.join(METADATA_DIR).join(CONFIG_FILE).exists() {
                return Self::open(directory);
            }
            current = directory.parent();
        }
        Err(WaymarkError::project_not_found(start.to_string()))
    }

    /// The project root: the directory containing `.waymark/`.
    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    /// The metadata directory, `.waymark/`.
    pub fn metadata_dir(&self) -> Utf8PathBuf {
        self.root.join(METADATA_DIR)
    }

    /// The configuration document.
    pub fn config_path(&self) -> Utf8PathBuf {
        self.metadata_dir().join(CONFIG_FILE)
    }

    /// The store-wide write lock file.
    pub fn lock_path(&self) -> Utf8PathBuf {
        self.metadata_dir().join(crate::lock::LOCK_FILE)
    }

    /// The loaded configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Replace the in-memory configuration (used by configuration writes).
    pub fn set_config(&mut self, config: Config) {
        self.config = Arc::new(config);
    }

    /// The directory of `category`; `CONFIG_INVALID` when it is not declared.
    pub fn category_dir(&self, category: &CategoryKey) -> Result<Utf8PathBuf, WaymarkError> {
        self.config
            .category_dir(&self.root, category)
            .ok_or_else(|| self.undeclared_category(category))
    }

    /// The category declaration for `category`.
    pub fn category(&self, category: &CategoryKey) -> Option<&Category> {
        self.config.categories.get(category)
    }

    /// The document path for (`id`, `category`).
    ///
    /// The file name comes from the ID alone, so a title or display-name change
    /// never moves a file.
    pub fn path_for(
        &self,
        id: &WorkItemId,
        category: &CategoryKey,
    ) -> Result<Utf8PathBuf, WaymarkError> {
        let directory = self.category_dir(category)?;
        Ok(directory.join(id.file_name()))
    }

    /// Map a document path back to the category that owns it, if any.
    ///
    /// The longest matching category directory wins, so a hand-edited
    /// configuration with nested paths still classifies deterministically.
    pub fn category_for_path(&self, path: &Utf8Path) -> Option<CategoryKey> {
        let mut best: Option<(usize, CategoryKey)> = None;
        for (key, category) in &self.config.categories {
            let directory = self.root.join(&category.path);
            if path.starts_with(&directory) {
                let length = directory.as_str().len();
                if best.as_ref().is_none_or(|(known, _)| length > *known) {
                    best = Some((length, key.clone()));
                }
            }
        }
        best.map(|(_, key)| key)
    }

    /// Every document in the project, sorted by path.
    ///
    /// The walk skips symlinks, directories, non-`.md` files, and files whose
    /// stem is not the lowercase form of a stable ID; each skip is reported in
    /// [`Inventory::problems`] rather than aborting the scan, so one corrupt
    /// file never hides the rest of the project.
    pub fn inventory(&self) -> Result<Inventory, WaymarkError> {
        let mut inventory = Inventory::default();
        for (key, category) in &self.config.categories {
            let directory = self.root.join(&category.path);
            let entries = match fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    // A category directory that was never created is an empty
                    // category, not a failure.
                    continue;
                }
                Err(source) => {
                    return Err(WaymarkError::io(directory.to_string(), "read", &source));
                }
            };
            for entry in entries {
                let entry = entry
                    .map_err(|source| WaymarkError::io(directory.to_string(), "read", &source))?;
                self.inventory_entry(&mut inventory, key, &entry);
            }
        }
        inventory
            .entries
            .sort_by(|left, right| left.path.cmp(&right.path));
        inventory
            .problems
            .sort_by(|left, right| left.path.cmp(&right.path));
        Ok(inventory)
    }

    fn inventory_entry(
        &self,
        inventory: &mut Inventory,
        category: &CategoryKey,
        entry: &fs::DirEntry,
    ) {
        let raw_path = entry.path();
        let path = match Utf8PathBuf::from_path_buf(raw_path) {
            Ok(path) => path,
            Err(path) => {
                inventory.problems.push(InventoryProblem {
                    path: Utf8PathBuf::from(path.to_string_lossy().to_string()),
                    reason: "the file name is not UTF-8".to_owned(),
                });
                return;
            }
        };

        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(source) => {
                inventory.problems.push(InventoryProblem {
                    path,
                    reason: source.to_string(),
                });
                return;
            }
        };
        if file_type.is_symlink() {
            inventory.problems.push(InventoryProblem {
                path,
                reason: "symbolic links are not followed".to_owned(),
            });
            return;
        }
        if !file_type.is_file() || path.extension() != Some("md") {
            return;
        }

        let stem = path.file_stem().unwrap_or_default();
        let id = match WorkItemId::parse(stem) {
            Ok(id) if id.as_str() == stem => id,
            Ok(_) => {
                inventory.problems.push(InventoryProblem {
                    path,
                    reason: "the file name is not the lowercase form of its stable ID".to_owned(),
                });
                return;
            }
            Err(error) => {
                inventory.problems.push(InventoryProblem {
                    path,
                    reason: format!("the file name is not a stable ID: {error}"),
                });
                return;
            }
        };

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(source) => {
                inventory.problems.push(InventoryProblem {
                    path,
                    reason: source.to_string(),
                });
                return;
            }
        };

        inventory.entries.push(InventoryEntry {
            path,
            category: category.clone(),
            id,
            size: metadata.len(),
            modified: metadata.modified().ok().map(DateTime::<Utc>::from),
        });
    }

    /// Allocate a stable ID the current inventory does not already use.
    ///
    /// Collisions are detected against the inventory and regenerated before
    /// anything is written; exhausting the attempt budget is
    /// `ID_GENERATION_FAILED`.
    pub fn allocate_id(&self, inventory: &Inventory) -> Result<WorkItemId, WaymarkError> {
        let mut source = self
            .ids
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for _ in 0..MAX_ID_ATTEMPTS {
            let candidate = format!("{PREFIX}{}", source.next_candidate());
            let Ok(id) = WorkItemId::parse(&candidate) else {
                continue;
            };
            if !inventory.contains(&id) {
                return Ok(id);
            }
        }
        Err(WaymarkError::id_generation_failed(MAX_ID_ATTEMPTS))
    }

    /// Install a deterministic ID source.
    pub fn with_id_source(mut self, source: Box<dyn IdSource + Send>) -> Self {
        self.ids = Arc::new(Mutex::new(source));
        self
    }

    /// Install a clock.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// The clock this project hands to operations.
    pub fn clock(&self) -> &dyn Clock {
        self.clock.as_ref()
    }

    /// Read a document's bytes.
    pub fn read_bytes(&self, path: &Utf8Path) -> Result<Vec<u8>, WaymarkError> {
        let mut file = fs::File::open(path)
            .map_err(|source| WaymarkError::io(path.to_string(), "open", &source))?;
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut bytes)
            .map_err(|source| WaymarkError::io(path.to_string(), "read", &source))?;
        Ok(bytes)
    }

    /// The fingerprint of the document at `path`.
    pub fn fingerprint(&self, path: &Utf8Path) -> Result<Fingerprint, WaymarkError> {
        let bytes = self.read_bytes(path)?;
        Ok(Fingerprint::of(&bytes))
    }

    /// Resolve a stable ID to its document.
    ///
    /// Uses the inventory, so a duplicate claim or an ID/path mismatch is an
    /// error rather than a guess.
    pub fn locate(&self, id: &WorkItemId) -> Result<InventoryEntry, WaymarkError> {
        let inventory = self.inventory()?;
        let duplicates = inventory.duplicate_of(id);
        if !duplicates.is_empty() {
            return Err(WaymarkError::validation(
                format!("work item {id}"),
                vec![crate::error::ValidationIssue::new(
                    "id",
                    format!(
                        "the ID is claimed by {} files: {}",
                        duplicates.len(),
                        duplicates
                            .iter()
                            .map(|path| path.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )],
            ));
        }
        inventory
            .entry(id)
            .cloned()
            .ok_or_else(|| WaymarkError::item_not_found(id))
    }

    /// Resolve a stable ID to the document and its category, reading the
    /// document itself.
    pub fn load(
        &self,
        id: &WorkItemId,
    ) -> Result<(crate::model::WorkItem, InventoryEntry), WaymarkError> {
        let entry = self.locate(id)?;
        let item = crate::parse::parse_item_file(&entry.path)?;
        Ok((item, entry))
    }

    fn undeclared_category(&self, category: &CategoryKey) -> WaymarkError {
        WaymarkError::config_invalid(
            self.config_path().to_string(),
            vec![format!(
                "categories.{category}: the category is not declared in the project configuration"
            )],
        )
    }
}

/// Ensure `.waymark/.gitignore` ignores every entry in `entries`, creating or
/// appending as needed. A symlinked `.gitignore` is refused, not written
/// through.
pub fn ensure_gitignore(metadata_dir: &Utf8Path, entries: &[&str]) -> Result<(), WaymarkError> {
    let path = metadata_dir.join(".gitignore");
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() {
            return Err(WaymarkError::lock_failed(
                path.to_string(),
                "the .gitignore is a symbolic link and will not be written through",
            ));
        }
    }
    let mut contents = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(source) => return Err(WaymarkError::io(path.to_string(), "read", &source)),
    };
    let mut changed = false;
    for entry in entries {
        if contents.lines().any(|line| line.trim() == *entry) {
            continue;
        }
        if !contents.is_empty() && !contents.ends_with('\n') {
            contents.push('\n');
        }
        contents.push_str(entry);
        contents.push('\n');
        changed = true;
    }
    if changed {
        crate::write::replace_unchecked(&path, contents.as_bytes())?;
    }
    Ok(())
}

/// Resolve a possibly-relative path against the working directory without
/// requiring the path or the directory to exist.
fn absolute(path: &Utf8Path) -> Utf8PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    let Ok(cwd) = std::env::current_dir() else {
        return path.to_path_buf();
    };
    let base = Utf8PathBuf::from_path_buf(cwd).unwrap_or_else(|_| Utf8PathBuf::from("/"));
    normalize(&base.join(path))
}

/// Lexically normalize `.` and `..` components. `discover` may be handed a path
/// that does not exist yet, so this cannot canonicalize.
fn normalize(path: &Utf8Path) -> Utf8PathBuf {
    let mut out = Utf8PathBuf::new();
    for component in path.components() {
        match component {
            camino::Utf8Component::CurDir => {}
            camino::Utf8Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_str()),
        }
    }
    out
}
