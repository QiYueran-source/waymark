//! The atomic write engine and the project write lock.
//!
//! Two mechanisms protect the store, and they answer different questions:
//!
//! - the **lock** serializes writers, so two operations cannot interleave their
//!   read-modify-write cycles;
//! - the **fingerprint** detects a change a writer did not make, so an external
//!   edit is reported as `WRITE_CONFLICT` instead of being silently overwritten.
//!
//! Neither is sufficient alone. The lock cannot see an editor, and a fingerprint
//! cannot stop two of our own operations from racing. Together they keep the
//! invariant the spec pins: no write ever replaces bytes it did not read.
//!
//! # Ordering
//!
//! A mediated write is always: validate → fingerprint check → temp file in the
//! *same directory* → `fsync` → `rename`. The rename is the only step a reader
//! can observe, so a reader sees either the old bytes or the new bytes.

use std::fs;
use std::io::Write;
use std::time::{Duration, Instant};

use camino::Utf8Path;

use crate::error::WaymarkError;

pub use crate::store::Fingerprint;

/// How long a mediated write waits for the lock by default.
pub const DEFAULT_LOCK_WAIT: Duration = Duration::from_secs(10);

/// The pause between lock attempts.
///
/// Exponential backoff keeps a shower of quick operations from spinning, while
/// the cap keeps acquisition latency for a legitimate short contention under a
/// hundred milliseconds.
const LOCK_BACKOFF_START: Duration = Duration::from_millis(2);
const LOCK_BACKOFF_CAP: Duration = Duration::from_millis(50);

/// Write `bytes` to `path` atomically, without a fingerprint check.
///
/// Use this for files that have no prior content to conflict with (a fresh
/// configuration, the ignore file) and for the low-level half of a mediated
/// write. It is never the whole of a work-item write.
pub fn replace_unchecked(path: &Utf8Path, bytes: &[u8]) -> Result<Fingerprint, WaymarkError> {
    write_atomic(path, bytes)
}

/// Write `bytes` to `path` atomically, but only if the file still has
/// `expected` as its fingerprint.
///
/// `id` is only used to shape the conflict error.
pub fn replace_checked(
    path: &Utf8Path,
    bytes: &[u8],
    expected: &Fingerprint,
    id: &crate::id::WorkItemId,
) -> Result<Fingerprint, WaymarkError> {
    let actual = match fs::read(path) {
        Ok(current) => Fingerprint::of(&current),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(WaymarkError::write_conflict(
                id,
                path.to_string(),
                expected.as_str(),
                "missing",
            ));
        }
        Err(source) => return Err(WaymarkError::io(path.to_string(), "read", &source)),
    };
    if actual != *expected {
        return Err(WaymarkError::write_conflict(
            id,
            path.to_string(),
            expected.as_str(),
            actual.as_str(),
        ));
    }
    write_atomic(path, bytes)
}

/// One document in an all-or-nothing batch.
///
/// `expected` is the fingerprint the caller read; `bytes` is the complete new
/// document. A batch target never describes a partial change.
#[derive(Debug, Clone, Copy)]
pub struct BatchTarget<'a> {
    /// The item the document belongs to. Only used to shape conflict errors.
    pub id: &'a crate::id::WorkItemId,
    /// The document to replace.
    pub path: &'a Utf8Path,
    /// The bytes that must currently be on disk.
    pub expected: &'a Fingerprint,
    /// The complete new document.
    pub bytes: &'a [u8],
}

/// One document a batch replaced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchApplied {
    /// The item the document belongs to, when it is a work-item document.
    pub id: Option<crate::id::WorkItemId>,
    /// The document that changed.
    pub path: camino::Utf8PathBuf,
    /// The fingerprint of the bytes now on disk.
    pub fingerprint: Fingerprint,
}

/// One non-item document in an all-or-nothing batch.
///
/// The configuration document is the only current use: it is replaced with the
/// work-item documents in the same batch, but it has no stable work-item ID, so
/// it cannot be a [`BatchTarget`].
#[derive(Debug, Clone, Copy)]
pub struct FileTarget<'a> {
    /// The document to replace.
    pub path: &'a Utf8Path,
    /// The bytes that must currently be on disk.
    pub expected: &'a Fingerprint,
    /// The complete new document.
    pub bytes: &'a [u8],
}

/// One internal batch target: a work-item document or, when `id` is `None`, a
/// non-item document such as the configuration.
#[derive(Debug, Clone, Copy)]
struct RawTarget<'a> {
    id: Option<&'a crate::id::WorkItemId>,
    path: &'a Utf8Path,
    expected: &'a Fingerprint,
    bytes: &'a [u8],
}

impl<'a> From<&'a BatchTarget<'a>> for RawTarget<'a> {
    fn from(target: &'a BatchTarget<'a>) -> Self {
        Self {
            id: Some(target.id),
            path: target.path,
            expected: target.expected,
            bytes: target.bytes,
        }
    }
}

impl<'a> From<&'a FileTarget<'a>> for RawTarget<'a> {
    fn from(target: &'a FileTarget<'a>) -> Self {
        Self {
            id: None,
            path: target.path,
            expected: target.expected,
            bytes: target.bytes,
        }
    }
}

/// Replace several documents as one application-level all-or-nothing batch.
///
/// # Contract
///
/// 1. **Prevalidate everything first.** Each target must be a regular file (a
///    symlink or directory is refused), its current bytes must match `expected`,
///    and no path may appear twice. Any violation fails before a byte is staged,
///    so nothing is written.
/// 2. **Stage everything.** Every new document is written and `fsync`ed to a
///    temporary file in its destination directory before *any* target is
///    replaced. A staging failure removes the staged files and leaves the store
///    untouched.
/// 3. **Replace in order.** Each target is replaced by renaming its staged
///    temporary over the destination, so a reader sees old bytes or new bytes,
///    never a blend.
/// 4. **Roll back on failure.** If a replace fails, every already-replaced
///    document is restored from the bytes captured during prevalidation. If a
///    restore also fails the operation returns `ROLLBACK_FAILED` naming the
///    applied, failed, and not-restored targets — it never claims success.
///
/// The caller must hold the project write lock for the whole call. The batch
/// engine does not take it, because one operation may need to do more work
/// (validation that reads the project, projection refresh) under the same lock.
///
/// This is not a filesystem multi-file transaction: a crash between two renames
/// can still leave the new bytes of the earlier documents in place. The contract
/// is about *reported* failure, which is what the spec pins.
pub fn replace_batch(targets: &[BatchTarget<'_>]) -> Result<Vec<BatchApplied>, WaymarkError> {
    let raw: Vec<RawTarget<'_>> = targets.iter().map(RawTarget::from).collect();
    run_batch(&raw)
}

/// Replace work-item documents and one non-item document as one batch.
///
/// This is the migration scaffold's writer: a schema or version migration
/// changes the configuration and the documents together, so a failure on either
/// side writes neither. A conflict on the non-item document is reported as a
/// `WRITE_CONFLICT` with a null `id`; migration callers translate any execution
/// failure into `MIGRATION_FAILED` with per-target outcomes.
pub fn replace_batch_with_file(
    targets: &[BatchTarget<'_>],
    file: Option<FileTarget<'_>>,
) -> Result<Vec<BatchApplied>, WaymarkError> {
    let mut raw: Vec<RawTarget<'_>> = targets.iter().map(RawTarget::from).collect();
    if let Some(file) = &file {
        raw.push(RawTarget::from(file));
    }
    run_batch(&raw)
}

fn run_batch(targets: &[RawTarget<'_>]) -> Result<Vec<BatchApplied>, WaymarkError> {
    if targets.is_empty() {
        return Ok(Vec::new());
    }

    // 1. Prevalidate, and capture the bytes every rollback would restore.
    let mut originals: Vec<(camino::Utf8PathBuf, Vec<u8>)> = Vec::with_capacity(targets.len());
    let mut seen: std::collections::BTreeSet<&Utf8Path> = std::collections::BTreeSet::new();
    for target in targets {
        if !seen.insert(target.path) {
            return Err(WaymarkError::internal(format!(
                "the batch names {} twice; a document may be replaced only once per batch",
                target.path
            )));
        }
        validate_batch_target(target)?;
        let original = fs::read(target.path)
            .map_err(|source| WaymarkError::io(target.path.to_string(), "read", &source))?;
        let actual = Fingerprint::of(&original);
        if actual != *target.expected {
            return Err(conflict(target, actual.as_str()));
        }
        originals.push((target.path.to_path_buf(), original));
    }

    // 2. Stage every new document before touching a destination.
    let mut temps: Vec<StagedTemp> = Vec::with_capacity(targets.len());
    for target in targets {
        match stage(target.path, target.bytes) {
            Ok(temp) => temps.push(temp),
            Err(error) => {
                // Dropping a `NamedTempFile` removes it, so a staging failure
                // leaves no debris behind.
                drop(temps);
                return Err(error);
            }
        }
    }

    // 3. Replace in order, remembering how far we got.
    let mut applied: Vec<BatchApplied> = Vec::with_capacity(targets.len());
    let mut failure: Option<(usize, WaymarkError)> = None;
    for (index, (target, temp)) in targets.iter().zip(temps).enumerate() {
        match persist(temp, target.path) {
            Ok(()) => applied.push(BatchApplied {
                id: target.id.cloned(),
                path: target.path.to_path_buf(),
                fingerprint: Fingerprint::of(target.bytes),
            }),
            Err(error) => {
                failure = Some((index, error));
                break;
            }
        }
    }

    // 4. Roll back, or report the committed batch.
    let Some((failed_index, error)) = failure else {
        return Ok(applied);
    };

    // Only the documents replaced *before* the failure need restoring; the
    // failing one was never renamed into place.
    let rollback_failed = restore(&originals, failed_index);
    if rollback_failed.is_empty() {
        return Err(error);
    }
    Err(WaymarkError::rollback_failed(
        applied.iter().map(|entry| entry.path.to_string()).collect(),
        vec![targets[failed_index].path.to_string()],
        rollback_failed,
    ))
}

/// The conflict for a target whose bytes moved on since they were read.
///
/// A work-item document names its item. A non-item target (the configuration)
/// has no stable ID, so its conflict carries a null `id`; migration callers
/// translate it into `MIGRATION_FAILED` before it reaches an adapter.
fn conflict(target: &RawTarget<'_>, actual: &str) -> WaymarkError {
    match target.id {
        Some(id) => WaymarkError::write_conflict(
            id,
            target.path.to_string(),
            target.expected.as_str(),
            actual,
        ),
        None => WaymarkError::new(
            crate::error::Code::WriteConflict,
            format!(
                "{} changed on disk since it was read; reread and retry",
                target.path
            ),
        )
        .with_details(serde_json::json!({
            "id": serde_json::Value::Null,
            "path": target.path.to_string(),
            "expected_fingerprint": target.expected.as_str(),
            "actual_fingerprint": actual,
        })),
    }
}

/// Put the first `replaced` captured documents back, and report the ones that
/// could not be restored.
///
/// A restore itself goes through [`write_atomic`], so it inherits the same
/// symlink and regular-file refusals: a document that vanished or became a
/// directory after the batch started is reported, never silently skipped.
fn restore(originals: &[(camino::Utf8PathBuf, Vec<u8>)], replaced: usize) -> Vec<String> {
    let mut failed: Vec<String> = Vec::new();
    for (path, original) in originals.iter().take(replaced) {
        if write_atomic(path, original).is_err() {
            failed.push(path.to_string());
        }
    }
    failed
}

/// A new document already written and synced, not yet renamed into place.
#[derive(Debug)]
struct StagedTemp {
    temp: tempfile::NamedTempFile,
}

/// Refuse targets that cannot be replaced safely.
fn validate_batch_target(target: &RawTarget<'_>) -> Result<(), WaymarkError> {
    match fs::symlink_metadata(target.path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(WaymarkError::io(
            target.path.to_string(),
            "write",
            &std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "the destination is a symbolic link",
            ),
        )),
        Ok(metadata) if !metadata.is_file() => Err(WaymarkError::io(
            target.path.to_string(),
            "write",
            &std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "the destination is not a regular file",
            ),
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(conflict(target, "missing"))
        }
        Err(source) => Err(WaymarkError::io(target.path.to_string(), "read", &source)),
    }
}

/// Write `bytes` to a temporary file beside `path` and sync it.
fn stage(path: &Utf8Path, bytes: &[u8]) -> Result<StagedTemp, WaymarkError> {
    let directory = path.parent().unwrap_or_else(|| Utf8Path::new("."));
    let mut temp = tempfile::Builder::new()
        .prefix(".waymark-batch")
        .tempfile_in(directory)
        .map_err(|source| {
            WaymarkError::io(directory.to_string(), "create a temporary file in", &source)
        })?;
    temp.write_all(bytes)
        .map_err(|source| WaymarkError::io(path.to_string(), "write", &source))?;
    temp.flush()
        .map_err(|source| WaymarkError::io(path.to_string(), "flush", &source))?;
    temp.as_file()
        .sync_all()
        .map_err(|source| WaymarkError::io(path.to_string(), "fsync", &source))?;
    set_mode(temp.path(), existing_mode(path).unwrap_or(0o644))?;
    Ok(StagedTemp { temp })
}

/// Rename a staged document over its destination and sync the directory.
fn persist(staged: StagedTemp, path: &Utf8Path) -> Result<(), WaymarkError> {
    staged
        .temp
        .persist(path)
        .map_err(|error| WaymarkError::io(path.to_string(), "replace", &error.error))?;
    if let Some(directory) = path.parent() {
        if let Ok(handle) = fs::File::open(directory) {
            let _ = handle.sync_all();
        }
    }
    Ok(())
}

/// Replace `path` with `bytes` through a temporary file in the same directory.
///
/// The temporary file is created in the destination directory on purpose: a
/// rename across filesystems is not atomic, and `/tmp` is often a different
/// filesystem. The file mode is inherited from an existing target, else
/// `0644`.
fn write_atomic(path: &Utf8Path, bytes: &[u8]) -> Result<Fingerprint, WaymarkError> {
    let directory = path.parent().unwrap_or_else(|| Utf8Path::new("."));
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() {
            // Writing through a symlink would replace the link target, which is
            // how a project escapes its own root. Refuse instead.
            return Err(WaymarkError::io(
                path.to_string(),
                "write",
                &std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "the destination is a symbolic link",
                ),
            ));
        }
        if !metadata.is_file() {
            return Err(WaymarkError::io(
                path.to_string(),
                "write",
                &std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "the destination is not a regular file",
                ),
            ));
        }
    }

    let mut temp = tempfile::Builder::new()
        .prefix(".waymark-tmp")
        .tempfile_in(directory)
        .map_err(|source| {
            WaymarkError::io(directory.to_string(), "create a temporary file in", &source)
        })?;

    temp.write_all(bytes)
        .map_err(|source| WaymarkError::io(path.to_string(), "write", &source))?;
    temp.flush()
        .map_err(|source| WaymarkError::io(path.to_string(), "flush", &source))?;
    temp.as_file()
        .sync_all()
        .map_err(|source| WaymarkError::io(path.to_string(), "fsync", &source))?;

    let mode = existing_mode(path).unwrap_or(0o644);
    set_mode(temp.path(), mode)?;

    temp.persist(path)
        .map_err(|error| WaymarkError::io(path.to_string(), "replace", &error.error))?;

    // The rename is durable once the directory entry is synced. Failure to sync
    // the directory does not undo a committed write, so this is best-effort: a
    // filesystem that cannot sync a directory (some network mounts) still keeps
    // the rename.
    if let Ok(handle) = fs::File::open(directory) {
        let _ = handle.sync_all();
    }

    Ok(Fingerprint::of(bytes))
}

#[cfg(unix)]
fn existing_mode(path: &Utf8Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions().mode())
}

#[cfg(not(unix))]
fn existing_mode(_path: &Utf8Path) -> Option<u32> {
    None
}

#[cfg(unix)]
fn set_mode(path: &std::path::Path, mode: u32) -> Result<(), WaymarkError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|source| WaymarkError::io(path.display().to_string(), "chmod", &source))
}

#[cfg(not(unix))]
fn set_mode(_path: &std::path::Path, _mode: u32) -> Result<(), WaymarkError> {
    Ok(())
}

/// How long a lock acquisition should wait, and whether it should wait at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockWait {
    timeout: Duration,
}

impl LockWait {
    /// Wait up to `timeout`. `Duration::ZERO` means fail immediately when the
    /// lock is held.
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// The default policy: wait up to ten seconds.
    pub fn default_policy() -> Self {
        Self::new(DEFAULT_LOCK_WAIT)
    }

    /// Do not wait: a held lock is an immediate `LOCK_TIMEOUT`.
    pub fn immediate() -> Self {
        Self::new(Duration::ZERO)
    }

    /// The configured timeout.
    pub fn timeout(self) -> Duration {
        self.timeout
    }
}

impl Default for LockWait {
    fn default() -> Self {
        Self::default_policy()
    }
}

/// A held store-wide write lock, visible to the body of
/// [`with_write_lock`].
///
/// The handle exists only inside the closure: the guard borrows the lock, so
/// there is no way to return one and forget to release it. The lock is *not*
/// re-entrant — taking it twice inside one operation waits for itself, which the
/// bounded wait turns into a `LOCK_TIMEOUT` rather than a hang.
#[derive(Debug)]
pub struct LockGuard<'a> {
    path: camino::Utf8PathBuf,
    _guard: fd_lock::RwLockWriteGuard<'a, fs::File>,
}

impl LockGuard<'_> {
    /// The lock file path.
    pub fn path(&self) -> &Utf8Path {
        &self.path
    }
}

/// Take the lock at `path`, run `body`, and release the lock afterwards — even
/// when `body` returns an error.
///
/// Acquisition waits per `wait`; `LockWait::immediate()` reports contention as
/// `LOCK_TIMEOUT` immediately.
pub fn with_write_lock<T>(
    path: &Utf8Path,
    wait: LockWait,
    body: impl FnOnce(&LockGuard<'_>) -> Result<T, WaymarkError>,
) -> Result<T, WaymarkError> {
    let file = open_lock_file(path)?;
    let mut lock = fd_lock::RwLock::new(file);

    let started = Instant::now();
    let mut backoff = LOCK_BACKOFF_START;
    let guard = loop {
        match lock.try_write() {
            Ok(guard) => break guard,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                let waited = started.elapsed();
                if waited >= wait.timeout() {
                    return Err(WaymarkError::lock_timeout(
                        path.to_string(),
                        u64::try_from(waited.as_millis()).unwrap_or(u64::MAX),
                    ));
                }
                let remaining = wait.timeout().saturating_sub(waited);
                std::thread::sleep(backoff.min(remaining));
                backoff = (backoff * 2).min(LOCK_BACKOFF_CAP);
            }
            Err(source) => {
                return Err(WaymarkError::lock_failed(
                    path.to_string(),
                    source.to_string(),
                ));
            }
        }
    };

    let handle = LockGuard {
        path: path.to_path_buf(),
        _guard: guard,
    };
    body(&handle)
}

/// Whether `path` is currently locked by another open file description.
///
/// Used by tests that must prove a write happened while the lock was held; it is
/// not part of the write path.
pub fn is_locked(path: &Utf8Path) -> Result<bool, WaymarkError> {
    let file = open_lock_file(path)?;
    let mut lock = fd_lock::RwLock::new(file);
    let held = match lock.try_write() {
        Ok(_guard) => false,
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => true,
        Err(source) => {
            return Err(WaymarkError::lock_failed(
                path.to_string(),
                source.to_string(),
            ))
        }
    };
    Ok(held)
}

/// Open (or create) the lock file, refusing symlinks and non-regular files.
fn open_lock_file(path: &Utf8Path) -> Result<fs::File, WaymarkError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|source| WaymarkError::io(parent.to_string(), "create", &source))?;
    }

    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // `O_NOFOLLOW` refuses a symlinked lock path outright. `truncate(false)`
        // above matters as much: truncating the lock file of a concurrent
        // process would clear its lock on some filesystems.
        options.custom_flags(libc::O_NOFOLLOW);
        options.mode(0o644);
    }

    let file = options.open(path).map_err(|source| {
        if source.raw_os_error() == Some(libc::ELOOP) {
            WaymarkError::lock_failed(
                path.to_string(),
                "the lock path is a symbolic link and will not be followed",
            )
        } else {
            WaymarkError::lock_failed(path.to_string(), source.to_string())
        }
    })?;

    let metadata = file
        .metadata()
        .map_err(|source| WaymarkError::lock_failed(path.to_string(), source.to_string()))?;
    if !metadata.is_file() {
        return Err(WaymarkError::lock_failed(
            path.to_string(),
            "the lock path is not a regular file",
        ));
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    fn scratch() -> tempfile::TempDir {
        tempfile::tempdir().expect("a temporary directory")
    }

    fn lock_path(dir: &tempfile::TempDir) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(dir.path().join("write.lock")).expect("a UTF-8 path")
    }

    #[test]
    fn the_lock_is_free_again_after_the_body_returns() {
        let dir = scratch();
        let path = lock_path(&dir);
        with_write_lock(&path, LockWait::default_policy(), |guard| {
            assert!(guard.path().ends_with("write.lock"));
            Ok(())
        })
        .expect("the body succeeds");
        assert!(!is_locked(&path).expect("probing the lock succeeds"));
    }

    #[test]
    fn the_lock_is_released_when_the_body_fails() {
        let dir = scratch();
        let path = lock_path(&dir);
        let error = with_write_lock(&path, LockWait::default_policy(), |_guard| {
            Err::<(), _>(WaymarkError::internal("boom"))
        })
        .expect_err("the body fails");
        assert_eq!(error.code(), crate::error::Code::InternalError);
        assert!(!is_locked(&path).expect("probing the lock succeeds"));
    }

    #[test]
    fn a_held_lock_is_visible_as_held() {
        let dir = scratch();
        let path = lock_path(&dir);
        with_write_lock(&path, LockWait::default_policy(), |_guard| {
            assert!(is_locked(&path).expect("probing the lock succeeds"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn an_immediate_wait_reports_contention_instead_of_blocking() {
        let dir = scratch();
        let path = lock_path(&dir);
        with_write_lock(&path, LockWait::default_policy(), |_guard| {
            let error = with_write_lock(&path, LockWait::immediate(), |_inner| Ok(()))
                .expect_err("the lock is held for the duration of the body");
            assert_eq!(error.code(), crate::error::Code::LockTimeout);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_bounded_wait_gives_up_on_contention() {
        let dir = scratch();
        let path = lock_path(&dir);
        with_write_lock(&path, LockWait::default_policy(), |_guard| {
            let started = Instant::now();
            let error =
                with_write_lock(&path, LockWait::new(Duration::from_millis(50)), |_inner| {
                    Ok(())
                })
                .expect_err("the lock stays held");
            assert_eq!(error.code(), crate::error::Code::LockTimeout);
            assert!(started.elapsed() >= Duration::from_millis(50));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_symlinked_lock_path_is_refused() {
        let dir = scratch();
        let real = Utf8PathBuf::from_path_buf(dir.path().join("real.lock")).unwrap();
        fs::write(&real, b"").unwrap();
        let link = lock_path(&dir);
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let error = with_write_lock(&link, LockWait::immediate(), |_guard| Ok(()))
            .expect_err("a symlinked lock path is refused");
        assert_eq!(error.code(), crate::error::Code::LockFailed);
    }

    #[test]
    fn a_directory_in_place_of_the_lock_file_is_refused() {
        let dir = scratch();
        let path = lock_path(&dir);
        fs::create_dir(&path).unwrap();
        let error = with_write_lock(&path, LockWait::immediate(), |_guard| Ok(()))
            .expect_err("a directory is not a lock file");
        assert_eq!(error.code(), crate::error::Code::LockFailed);
    }

    #[test]
    fn an_atomic_replace_creates_then_rewrites_a_file() {
        let dir = scratch();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("item.md")).unwrap();
        let first = replace_unchecked(&path, b"one\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "one\n");
        let second = replace_checked(
            &path,
            b"two\n",
            &first,
            &crate::id::WorkItemId::parse("wm-3f7k9q2m4x8c").unwrap(),
        )
        .unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read_to_string(&path).unwrap(), "two\n");
    }

    #[test]
    fn an_atomic_replace_preserves_the_existing_file_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("item.md")).unwrap();
        replace_unchecked(&path, b"one\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        replace_unchecked(&path, b"two\n").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn an_atomic_replace_gives_a_new_file_the_standard_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("item.md")).unwrap();
        replace_unchecked(&path, b"one\n").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
    }

    #[test]
    fn a_stale_fingerprint_is_a_write_conflict_and_leaves_the_file_alone() {
        let dir = scratch();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("item.md")).unwrap();
        let stale = replace_unchecked(&path, b"one\n").unwrap();
        replace_unchecked(&path, b"edited elsewhere\n").unwrap();
        let error = replace_checked(
            &path,
            b"two\n",
            &stale,
            &crate::id::WorkItemId::parse("wm-3f7k9q2m4x8c").unwrap(),
        )
        .expect_err("the fingerprint no longer matches");
        assert_eq!(error.code(), crate::error::Code::WriteConflict);
        assert_eq!(fs::read_to_string(&path).unwrap(), "edited elsewhere\n");
    }

    #[test]
    fn a_write_through_a_symlink_is_refused() {
        let dir = scratch();
        let target = Utf8PathBuf::from_path_buf(dir.path().join("target.md")).unwrap();
        fs::write(&target, b"original\n").unwrap();
        let link = Utf8PathBuf::from_path_buf(dir.path().join("item.md")).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let error = replace_unchecked(&link, b"replacement\n").unwrap_err();
        assert_eq!(error.code(), crate::error::Code::IoError);
        assert_eq!(fs::read_to_string(&target).unwrap(), "original\n");
    }

    fn item_id() -> crate::id::WorkItemId {
        crate::id::WorkItemId::parse("wm-000000000001").unwrap()
    }

    fn document_path(dir: &tempfile::TempDir, name: &str) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(dir.path().join(name)).unwrap()
    }

    #[test]
    fn a_batch_replaces_every_target() {
        let dir = scratch();
        let first = document_path(&dir, "first.md");
        let second = document_path(&dir, "second.md");
        fs::write(&first, b"one\n").unwrap();
        fs::write(&second, b"two\n").unwrap();
        let id = item_id();

        let applied = replace_batch(&[
            BatchTarget {
                id: &id,
                path: &first,
                expected: &Fingerprint::of(b"one\n"),
                bytes: b"ONE\n",
            },
            BatchTarget {
                id: &id,
                path: &second,
                expected: &Fingerprint::of(b"two\n"),
                bytes: b"TWO\n",
            },
        ])
        .expect("both documents are replaced");

        assert_eq!(applied.len(), 2);
        assert_eq!(applied[1].fingerprint, Fingerprint::of(b"TWO\n"));
        assert_eq!(fs::read_to_string(&first).unwrap(), "ONE\n");
        assert_eq!(fs::read_to_string(&second).unwrap(), "TWO\n");
    }

    #[test]
    fn a_batch_with_a_stale_fingerprint_writes_nothing() {
        let dir = scratch();
        let first = document_path(&dir, "first.md");
        let second = document_path(&dir, "second.md");
        fs::write(&first, b"one\n").unwrap();
        fs::write(&second, b"moved on\n").unwrap();
        let id = item_id();

        let error = replace_batch(&[
            BatchTarget {
                id: &id,
                path: &first,
                expected: &Fingerprint::of(b"one\n"),
                bytes: b"ONE\n",
            },
            BatchTarget {
                id: &id,
                path: &second,
                expected: &Fingerprint::of(b"two\n"),
                bytes: b"TWO\n",
            },
        ])
        .expect_err("the second target conflicts");

        assert_eq!(error.code(), crate::error::Code::WriteConflict);
        assert_eq!(
            fs::read_to_string(&first).unwrap(),
            "one\n",
            "the first target was never touched"
        );
        assert_eq!(fs::read_to_string(&second).unwrap(), "moved on\n");
    }

    #[test]
    fn a_mid_batch_failure_restores_the_documents_already_replaced() {
        let dir = scratch();
        let first = document_path(&dir, "first.md");
        let second = document_path(&dir, "second.md");
        fs::write(&first, b"one\n").unwrap();
        // The second destination is a directory: the staged rename cannot replace
        // it, so the batch fails after the first document has been replaced.
        fs::create_dir(&second).unwrap();
        let id = item_id();

        let error = replace_batch(&[
            BatchTarget {
                id: &id,
                path: &first,
                expected: &Fingerprint::of(b"one\n"),
                bytes: b"ONE\n",
            },
            BatchTarget {
                id: &id,
                path: &second,
                expected: &Fingerprint::of(b"two\n"),
                bytes: b"TWO\n",
            },
        ])
        .expect_err("the directory cannot be replaced");

        assert_ne!(error.code(), crate::error::Code::RollbackFailed);
        assert_eq!(
            fs::read_to_string(&first).unwrap(),
            "one\n",
            "the replaced document was rolled back"
        );
    }

    #[test]
    fn a_batch_may_not_name_a_document_twice() {
        let dir = scratch();
        let path = document_path(&dir, "item.md");
        fs::write(&path, b"one\n").unwrap();
        let id = item_id();

        let error = replace_batch(&[
            BatchTarget {
                id: &id,
                path: &path,
                expected: &Fingerprint::of(b"one\n"),
                bytes: b"ONE\n",
            },
            BatchTarget {
                id: &id,
                path: &path,
                expected: &Fingerprint::of(b"ONE\n"),
                bytes: b"again\n",
            },
        ])
        .expect_err("the second entry would overwrite the first");

        assert_eq!(error.code(), crate::error::Code::InternalError);
        assert_eq!(fs::read_to_string(&path).unwrap(), "one\n");
    }

    #[test]
    fn an_empty_batch_is_a_successful_no_op() {
        assert!(replace_batch(&[]).unwrap().is_empty());
    }

    #[test]
    fn no_temporary_file_is_left_behind() {
        let dir = scratch();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("item.md")).unwrap();
        replace_unchecked(&path, b"one\n").unwrap();
        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["item.md".to_owned()]);
    }
}
