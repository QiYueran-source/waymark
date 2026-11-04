//! `waymark git …`: history and diff over the repository.
//!
//! This module is a thin re-export seam, not a second implementation: the rules
//! live in [`crate::git`], and the operations here are the entry point a CLI,
//! MCP, or Web adapter calls. They never write, never take the project lock, and
//! never keep a history of their own — the repository is the only history. When
//! Git is unavailable they succeed with an empty payload and one informational
//! finding per missing cause (`NOT_A_GIT_REPOSITORY`, `GIT_UNAVAILABLE`).

pub use crate::git::{
    diff, history, DiffBaseline, DiffReport, DiffRequest, DiffScope, GitProbe, HistoryReport,
    HistoryRequest, HistoryScope, Repository, DEFAULT_HISTORY_LIMIT,
};
