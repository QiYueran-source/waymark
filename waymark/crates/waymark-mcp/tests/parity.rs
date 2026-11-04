//! CLI/MCP capability parity.
//!
//! Every top-level CLI command family is reachable through at least one MCP
//! tool, and every MCP tool is reachable from a CLI family. The only deliberate
//! exceptions are commands that are adapter presentations of a capability the
//! tool surface already exposes, or that are not business capabilities at all;
//! each is named in `docs/implementation-decisions.md`.

mod common;

use std::collections::BTreeSet;

/// Top-level CLI commands and the MCP tools that cover them.
const FAMILIES: &[(&str, &[&str])] = &[
    ("init", &["waymark_project_init"]),
    ("create", &["waymark_create_work_item"]),
    ("show", &["waymark_get_work_item"]),
    ("status", &["waymark_set_status"]),
    ("title", &["waymark_set_title"]),
    ("body", &["waymark_replace_body"]),
    ("field", &["waymark_update_custom_fields"]),
    ("owner", &["waymark_transfer_owner"]),
    ("move", &["waymark_move_work_item"]),
    (
        "dependency",
        &["waymark_add_dependency", "waymark_remove_dependency"],
    ),
    ("parent", &["waymark_set_parent", "waymark_clear_parent"]),
    (
        "link",
        &["waymark_add_soft_link", "waymark_remove_soft_link"],
    ),
    ("tree", &["waymark_get_tree"]),
    ("dep", &["waymark_get_dependency_tree"]),
    ("list", &["waymark_list_work_items"]),
    ("ready", &["waymark_get_ready_items"]),
    ("blocked", &["waymark_get_blocked_items"]),
    ("archive", &["waymark_archive_work_item"]),
    ("search", &["waymark_search_work_items"]),
    ("stats", &["waymark_get_stats"]),
    ("project", &["waymark_project_info"]),
    ("git", &["waymark_git_history", "waymark_git_diff"]),
    (
        "config",
        &[
            "waymark_config_read",
            "waymark_config_validate",
            "waymark_config_set",
            "waymark_config_add_category",
            "waymark_config_update_category",
            "waymark_config_remove_category",
            "waymark_config_add_field",
            "waymark_config_update_field",
        ],
    ),
    (
        "migrate",
        &[
            "waymark_migrate_version",
            "waymark_migrate_schema",
            "waymark_migrate_storage",
        ],
    ),
    ("doctor", &["waymark_doctor"]),
    ("validate", &["waymark_validate"]),
];

/// CLI commands deliberately not exposed as their own tool.
const EXCEPTIONS: &[(&str, &str)] = &[
    (
        "query",
        "covered by `waymark_list_work_items`'s structured filter document",
    ),
    (
        "start",
        "covered by `waymark_set_status` with `in_progress`",
    ),
    ("close", "covered by `waymark_set_status` with `closed`"),
    ("reopen", "covered by `waymark_set_status` with `open`"),
    (
        "agent-doc",
        "a generated CLI reference, not a business capability",
    ),
    (
        "version",
        "reports the build version and capabilities; it is not a business capability",
    ),
    (
        "serve",
        "the CLI entry point for the read-only Web adapter; it has no MCP tool",
    ),
];

fn cli_commands() -> Vec<String> {
    let output = std::process::Command::new(common::waymark_binary())
        .arg("--help")
        .output()
        .expect("the CLI binary runs");
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    let mut commands = Vec::new();
    let mut in_commands = false;
    for line in help.lines() {
        if line.trim() == "Commands:" {
            in_commands = true;
            continue;
        }
        if in_commands {
            if line.trim().is_empty() {
                break;
            }
            if let Some(name) = line.split_whitespace().next() {
                commands.push(name.to_owned());
            }
        }
    }
    commands
}

#[test]
fn every_cli_command_family_has_an_mcp_tool_and_vice_versa() {
    let commands = cli_commands();
    assert!(
        commands.len() >= 30,
        "the command list parsed from --help is implausible: {commands:?}"
    );

    let tools: BTreeSet<&str> = waymark_mcp::TOOLS.iter().copied().collect();
    let mut covered_tools = BTreeSet::new();

    for command in &commands {
        if let Some((_, tools_for_family)) = FAMILIES.iter().find(|(name, _)| name == command) {
            for tool in *tools_for_family {
                assert!(
                    tools.contains(tool),
                    "the `{command}` family maps to unknown tool {tool}"
                );
                covered_tools.insert(*tool);
            }
            continue;
        }
        assert!(
            EXCEPTIONS.iter().any(|(name, _)| name == command),
            "the CLI command `{command}` has no MCP tool family and is not a documented exception"
        );
    }

    // The reverse direction: every advertised tool is reachable from a CLI
    // command family, so the catalog has no orphan.
    let missing: Vec<&&str> = tools.difference(&covered_tools).collect();
    assert!(
        missing.is_empty(),
        "MCP tools with no CLI family: {missing:?}"
    );

    // The exception list is exact, so a new exception cannot slip in unwritten.
    for (command, _) in EXCEPTIONS {
        assert!(
            commands.iter().any(|name| name == command),
            "the documented exception `{command}` is not a CLI command"
        );
    }
}

#[test]
fn every_deliberate_exception_is_documented_in_the_decisions_record() {
    let decisions = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/implementation-decisions.md"),
    )
    .expect("the decisions record is readable");
    let phase_b = decisions
        .split("# T10 — MCP adapter (phase B)")
        .nth(1)
        .expect("the phase-B decisions section exists");
    for (command, reason) in EXCEPTIONS {
        assert!(
            phase_b.contains(command),
            "the exception `{command}` is not named in the phase-B decisions"
        );
        let _ = reason;
    }
}
