//! `waymark git …`: history and diff over the repository.
//!
//! This module is a thin re-export seam, not a second implementation: the rules
//! live in [`crate::git`], and the operations here are the entry point a CLI,
//! MCP, or Web adapter calls. They never write, never take the project lock, and
//! never keep a history of their own — the repository is the only history.

pub use crate::git::{
    diff, history, DiffBaseline, DiffReport, DiffRequest, DiffScope, GitProbe, HistoryReport,
    HistoryRequest, HistoryScope, Repository, DEFAULT_HISTORY_LIMIT,
};
