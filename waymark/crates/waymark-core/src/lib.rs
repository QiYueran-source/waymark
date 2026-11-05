//! Waymark domain library.
//!
//! Every rule lives here. The CLI, the MCP server, and the read-only Web view
//! are adapters over this crate and cannot diverge from it.
//!
//! # Layout
//!
//! - [`id`], [`time`], [`model`] — the domain vocabulary: stable IDs, clocks and
//!   timestamps, the work-item and configuration value types.
//! - [`serialize`], [`parse`] — the canonical document bytes and their inverse.
//! - [`store`], [`lock`], [`write`] — the file store: project discovery, path
//!   mapping, the store-wide write lock, and atomic replacement.
//! - [`config`] — the project configuration document and its validation.
//! - [`projection`] — the disposable, fingerprinted in-memory view of the files.
//! - [`ops`] — one module per capability family, each taking explicit options.
//! - [`error`], [`location`], [`envelope`] — the machine contract.
//! - [`testing`] — fixtures and deterministic clock/ID seams, public so every
//!   downstream crate's tests share them.
//! - [`view`] — the shared JSON renderers the CLI and MCP adapters publish, so
//!   one machine payload is stated once.

/// The default loopback port for the read-only Web view.
///
/// This is the single home for the value: `waymark-web` re-exports it as
/// `DEFAULT_PORT`, the Web server and launcher read it, and the CLI uses it in
/// both build variants so the basic build cannot drift.
pub const DEFAULT_PORT: u16 = 7331;

pub mod config;
pub mod envelope;
pub mod error;
pub mod git;
pub mod id;
pub mod location;
pub mod lock;
pub mod model;
pub mod ops;
pub mod parse;
pub mod projection;
pub mod query;
pub mod readiness;
pub mod search;
pub mod serialize;
pub mod stats;
pub mod store;
pub mod testing;
pub mod time;
pub mod user_config;
pub mod view;
pub mod write;

pub use config::Config;
pub use error::{Code, ExitClass, Severity, ValidationIssue, WaymarkError};
pub use git::{DiffReport, DiffRequest, GitProbe, HistoryReport, HistoryRequest, Repository};
pub use id::WorkItemId;
pub use location::{Location, LocationKind};
pub use model::{CategoryKey, CustomFields, FieldValue, Status, WorkItem};
pub use query::{Filters, Projection, Query, Sort, SortField, Window};
pub use readiness::{Graph, Readiness};
pub use store::{InitOptions, Project};
pub use time::{Clock, FixedClock, SystemClock, Timestamp};

#[cfg(test)]
mod tests {
    use super::DEFAULT_PORT;

    #[test]
    fn the_default_loopback_port_is_7331() {
        // The functional specification names 7331 as the observable default;
        // this literal is the independent source of truth that keeps the one
        // constant from drifting.
        assert_eq!(DEFAULT_PORT, 7331);
    }
}
