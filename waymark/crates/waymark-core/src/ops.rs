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

pub mod create;
pub mod list;
pub mod mediated;
pub mod read;
pub mod relationship;
pub mod status;

use std::sync::Arc;
use std::time::Duration;

use crate::lock::LockWait;
use crate::time::{Clock, SystemClock};

/// The default lock wait, mirrored here so an adapter can build options without
/// importing the lock module.
pub const DEFAULT_LOCK_WAIT: Duration = crate::write::DEFAULT_LOCK_WAIT;

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
            .finish_non_exhaustive()
    }
}
