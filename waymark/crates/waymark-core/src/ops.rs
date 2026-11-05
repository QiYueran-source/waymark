//! Structured operations: one module per capability family.
//!
//! Every operation:
//!
//! - takes [`OpOptions`], which carries the clock, the operator identity, the
//!   lock wait, and the dry-run flag — nothing reads the ambient environment;
//! - validates everything it can *before* touching the disk;
//! - takes the store-wide write lock for the whole read-modify-write cycle;
//! - re-checks the content fingerprint immediately before replacing bytes;
//! - returns the resulting document, so an adapter never re-reads to describe
//!   what just happened.
//!
//! The operations are the domain. Adapters decide where to print; they do not
//! decide what is allowed.

pub mod archive;
pub mod config;
pub mod create;
pub mod doctor;
pub mod edit;
pub mod git;
pub mod link;
pub mod list;
pub mod local_config;
pub mod mediated;
pub mod migrate;
pub mod r#move;
pub mod owner;
pub mod parent;
pub mod read;
pub mod relationship;
pub mod status;
pub mod tree;

use std::sync::Arc;
use std::time::Duration;

use crate::lock::LockWait;
use crate::store::Fingerprint;
use crate::time::{Clock, SystemClock};

/// The default lock wait, mirrored here so an adapter can build options without
/// importing the lock module.
pub const DEFAULT_LOCK_WAIT: Duration = crate::write::DEFAULT_LOCK_WAIT;

/// Translate a millisecond override into a lock-wait policy.
///
/// Both adapters expose the same `lock_timeout_ms` option, so the meaning is
/// shared: `None` keeps the default ten-second wait, `Some(0)` fails
/// immediately when the lock is held, and any other value bounds the wait.
pub fn lock_wait_from_millis(milliseconds: Option<u64>) -> LockWait {
    match milliseconds {
        Some(0) => LockWait::immediate(),
        Some(value) => LockWait::new(Duration::from_millis(value)),
        None => LockWait::default_policy(),
    }
}

/// Everything an operation needs from its caller that is not domain input.
#[derive(Clone)]
pub struct OpOptions {
    /// The clock used for every timestamp the operation writes.
    pub clock: Arc<dyn Clock>,
    /// The acting identity. Used only where the spec says it is recorded.
    pub operator: Option<String>,
    /// How long to wait for the store lock.
    pub lock_wait: LockWait,
    /// Validate and compute, but do not write.
    pub dry_run: bool,
    /// The configuration fingerprint a configuration write expects to find.
    ///
    /// `None` keeps the historical behaviour: a configuration write re-reads
    /// the document under the lock and rebases onto it. Supplying a fingerprint
    /// turns a concurrent hand edit into `WRITE_CONFLICT` instead. Other write
    /// families ignore it; they guard their own documents individually.
    pub expected_config_fingerprint: Option<Fingerprint>,
}

impl OpOptions {
    /// The production defaults: the system clock, no operator override, a ten
    /// second lock wait, and writes enabled.
    pub fn new() -> Self {
        Self {
            clock: Arc::new(SystemClock),
            operator: None,
            lock_wait: LockWait::default_policy(),
            dry_run: false,
            expected_config_fingerprint: None,
        }
    }

    /// Use `clock` for timestamps.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// Act as `operator`.
    pub fn with_operator(mut self, operator: impl Into<String>) -> Self {
        self.operator = Some(operator.into());
        self
    }

    /// Wait at most `wait` for the lock.
    pub fn with_lock_wait(mut self, wait: LockWait) -> Self {
        self.lock_wait = wait;
        self
    }

    /// Compute without writing.
    pub fn dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }

    /// Require the on-disk configuration to still have `fingerprint`.
    pub fn with_expected_config_fingerprint(mut self, fingerprint: Fingerprint) -> Self {
        self.expected_config_fingerprint = Some(fingerprint);
        self
    }

    /// The acting identity, remembered on the returned result.
    pub fn operator(&self) -> Option<&str> {
        self.operator.as_deref()
    }
}

impl Default for OpOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for OpOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpOptions")
            .field("operator", &self.operator)
            .field("lock_wait", &self.lock_wait)
            .field("dry_run", &self.dry_run)
            .field(
                "expected_config_fingerprint",
                &self.expected_config_fingerprint,
            )
            .finish_non_exhaustive()
    }
}
