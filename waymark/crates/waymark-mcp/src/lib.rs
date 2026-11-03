//! The Waymark MCP adapter over stdio.
//!
//! The adapter owns three things and nothing else:
//!
//! 1. **Transport**: MCP over stdio, via `rmcp`'s stdio transport and
//!    [`rmcp::ServiceExt::serve`]. There is no HTTP listener.
//! 2. **Representation**: the shared versioned envelope on every successful
//!    result and every business failure, with the shared registry codes and
//!    typed details.
//! 3. **Project context**: the working directory, or `--project DIR`.
//!
//! Every rule lives in `waymark-core`. If a rule appears here, it is in the
//! wrong crate.

pub mod envelope;
pub mod invariants;
pub mod server;
pub mod tools;

pub use server::WaymarkServer;

/// The phase-A tool catalog: project, configuration, diagnostics, and reads.
///
/// Phase B adds the write, lifecycle, relationship, archive, migration, Git, and
/// parity tools; the catalog test checks this list against the served tools so a
/// tool cannot be added without being named here.
pub const PHASE_A_TOOLS: &[&str] = &[
    "waymark_project_init",
    "waymark_project_info",
    "waymark_config_read",
    "waymark_config_validate",
    "waymark_validate",
    "waymark_doctor",
    "waymark_get_work_item",
    "waymark_list_work_items",
    "waymark_search_work_items",
    "waymark_get_tree",
    "waymark_get_dependency_tree",
    "waymark_get_ready_items",
    "waymark_get_blocked_items",
    "waymark_get_stats",
];
