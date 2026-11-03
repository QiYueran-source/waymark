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

/// The complete tool catalog, in the order it is described to a client.
///
/// Phase A contributed the project, configuration read/diagnostic, and read
/// tools; phase B contributes configuration writes, migration, Git, and every
/// single-target write. [`TOOLS`] is the whole surface; the catalog test
/// compares it with the served tools so a tool cannot be added without being
/// named here, and [`PHASE_A_TOOLS`] stays as the phase-A slice for the
/// decisions record.
pub const TOOLS: &[&str] = &[
    // Phase A: project, configuration reads, diagnostics, reads.
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
    // Phase B: configuration writes.
    "waymark_config_set",
    "waymark_config_add_category",
    "waymark_config_update_category",
    "waymark_config_remove_category",
    "waymark_config_add_field",
    "waymark_config_update_field",
    // Phase B: migration.
    "waymark_migrate_version",
    "waymark_migrate_schema",
    "waymark_migrate_storage",
    // Phase B: Git inspection.
    "waymark_git_history",
    "waymark_git_diff",
    // Phase B: work-item creation and editing.
    "waymark_create_work_item",
    "waymark_set_title",
    "waymark_replace_body",
    "waymark_update_custom_fields",
    "waymark_set_status",
];

/// The phase-A slice of [`TOOLS`], kept as the decisions record's reference.
///
/// Listed literally because a const slice cannot be built from a subslice of
/// another const slice on the workspace's Rust version; the catalog test asserts
/// that [`TOOLS`] starts with exactly this list.
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
