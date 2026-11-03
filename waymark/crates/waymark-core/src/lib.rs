//! Waymark domain library.
//!
//! Every rule lives here. The CLI, the MCP server, and the read-only Web view
//! are adapters over this crate and cannot diverge from it.

pub mod config;
pub mod id;
pub mod lock;
pub mod model;
pub mod ops;
pub mod parse;
pub mod serialize;
pub mod store;
pub mod testing;
pub mod time;

pub use id::WorkItemId;
pub use model::{Status, WorkItem};
pub use time::{Clock, FixedClock, SystemClock};
