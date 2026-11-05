//! The project write lock.
//!
//! The implementation lives in [`crate::write`] with the atomic replacement it
//! protects; this module is the public entry point adapters and operations use,
//! so the lock file name and the bounded-wait policy stay in one place.

use camino::Utf8Path;

use crate::error::WaymarkError;
use crate::store::Project;
pub use crate::write::{is_locked, with_write_lock, LockGuard, LockWait, DEFAULT_LOCK_WAIT};

/// The lock file name inside `.waymark/`.
pub const LOCK_FILE: &str = "write.lock";

/// Take the store-wide write lock for `project`, run `body`, and release it.
///
/// Acquisition repairs a missing `.waymark/.gitignore` entry first, for both the
/// lock file and the local user document, so neither ever shows up as an
/// untracked change and an unsafe ignore file stops the write before anything is
/// written.
pub fn with_project_lock<T>(
    project: &Project,
    wait: LockWait,
    body: impl FnOnce() -> Result<T, WaymarkError>,
) -> Result<T, WaymarkError> {
    crate::store::ensure_gitignore(
        &project.metadata_dir(),
        &[
            crate::store::LOCK_IGNORE_ENTRY,
            crate::user_config::USER_CONFIG_IGNORE_ENTRY,
        ],
    )?;
    with_write_lock(&project.lock_path(), wait, |_lock| body())
}

/// Whether the store lock at `root` is currently held.
pub fn is_store_locked(root: &Utf8Path) -> Result<bool, WaymarkError> {
    is_locked(&root.join(crate::config::METADATA_DIR).join(LOCK_FILE))
}
