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
//! - [`ops`] — one module per capability family, each taking explicit options.
//! - [`error`], [`location`], [`envelope`] — the machine contract.
//! - [`testing`] — fixtures and deterministic clock/ID seams, public so every
//!   downstream crate's tests share them.

pub mod config;
pub mod envelope;
pub mod error;
pub mod id;
pub mod location;
pub mod lock;
pub mod model;
pub mod ops;
pub mod parse;
pub mod serialize;
pub mod store;
pub mod testing;
pub mod time;
pub mod write;

pub use config::Config;
pub use error::{Code, ExitClass, Severity, ValidationIssue, WaymarkError};
pub use id::WorkItemId;
pub use location::{Location, LocationKind};
pub use model::{CategoryKey, CustomFields, FieldValue, Status, WorkItem};
pub use store::{InitOptions, Project};
pub use time::{Clock, FixedClock, SystemClock, Timestamp};
