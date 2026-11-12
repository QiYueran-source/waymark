//! CLI/MCP capability parity.
//!
//! Every top-level CLI command family is reachable through at least one MCP
//! tool, and every MCP tool is reachable from a CLI family. The only deliberate
//! exceptions are commands that are adapter presentations of a capability the
//! tool surface already exposes, or that are not business capabilities at all;
//! each is named in `docs/implementation-decisions.md`.

mod common;

use std::collections::BTreeSet;

use common::{act_as, act_as_in};
use serde_json::json;
use waymark_core::error::ExitClass;
use waymark_core::ops::relationship;
use waymark_core::WaymarkError;
use waymark_core::WorkItemId;

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

/// The CLI invocations that can rewrite a second document, as the path from the
/// top-level command to the verb.
const CROSS_ITEM_CLI: &[&[&str]] = &[
    &["create"],
    &["link", "add"],
    &["link", "remove"],
    &["archive"],
];

/// The MCP tools that can rewrite a second document, paired with the CLI
/// invocation that covers the same capability.
const CROSS_ITEM_TOOLS: &[(&str, &[&str])] = &[
    ("waymark_create_work_item", &["create"]),
    ("waymark_add_soft_link", &["link", "add"]),
    ("waymark_remove_soft_link", &["link", "remove"]),
    ("waymark_archive_work_item", &["archive"]),
];

fn run_help(arguments: &[&str]) -> String {
    let output = std::process::Command::new(common::waymark_binary())
        .args(arguments)
        .arg("--help")
        .output()
        .expect("the CLI binary runs");
    assert!(
        output.status.success(),
        "`{} --help` failed: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Every word `--help` prints, so a flag is found by its exact name only.
fn help_tokens(help: &str) -> BTreeSet<&str> {
    help.split(|character: char| character.is_whitespace() || character == ',')
        .filter(|token| !token.is_empty())
        .collect()
}

#[test]
fn every_cross_item_cli_operation_exposes_the_consent_flag() {
    for invocation in CROSS_ITEM_CLI {
        let help = run_help(invocation);
        let tokens = help_tokens(&help);
        assert!(
            tokens.contains("--cascade"),
            "`{}` must offer --cascade, the one consent flag: {help}",
            invocation.join(" ")
        );
        assert!(
            help.contains("--cascade"),
            "`{}` does not describe its consent flag: {help}",
            invocation.join(" ")
        );
    }
}

#[tokio::test]
async fn every_cross_item_mcp_tool_exposes_the_consent_flag() {
    let fixture = common::fixture(&[]);
    let client = common::Client::connect(fixture.root.clone()).await;
    for (tool_name, cli) in CROSS_ITEM_TOOLS {
        assert!(
            CROSS_ITEM_CLI.contains(cli),
            "{tool_name} names a CLI invocation the flag test does not check"
        );
        let tool = client
            .tools()
            .await
            .into_iter()
            .find(|tool| tool.name == *tool_name)
            .unwrap_or_else(|| panic!("{tool_name} is catalogued"));
        let schema = serde_json::Value::Object((*tool.input_schema).clone());
        assert_eq!(
            schema["properties"]["cascade"]["type"], "boolean",
            "{tool_name} must take `cascade`, the one consent flag: {schema}"
        );
        assert!(
            !schema["required"]
                .as_array()
                .expect("required is a list")
                .contains(&json!("cascade")),
            "{tool_name} must keep consent optional, as the CLI's flag is"
        );
    }
    client.shutdown().await;
}

/// The shape of a `_meta` warning both adapters put on a successful result.
fn warning_shape(warning: &serde_json::Value) -> (String, serde_json::Value) {
    (
        warning["code"].as_str().expect("a warning code").to_owned(),
        warning["location"].clone(),
    )
}

#[tokio::test]
async fn both_adapters_report_the_same_written_and_untouched_documents() {
    let fixture = common::fixture(&["one", "two", "three"]);
    let target = fixture.ids[0].clone();
    let dependent = fixture.ids[1].clone();
    let linked = fixture.ids[2].clone();
    let project = fixture.project();
    relationship::add_dependency(&project, &dependent, &target, &common::options())
        .expect("the dependency is added");
    waymark_core::ops::link::add_soft_link(
        &project,
        &linked,
        &target,
        &common::options().with_cascade().with_operator("frank"),
    )
    .expect("the link is added");
    waymark_core::ops::owner::transfer_owner(&project, &dependent, "alice", &common::options())
        .expect("the dependent belongs to someone else");
    // The target becomes someone else's too, so the archive carries the one
    // `OWNER_MISMATCH` warning both adapters have to render the same way.
    waymark_core::ops::owner::transfer_owner(&project, &target, "carol", &common::options())
        .expect("the target belongs to someone else");
    act_as(&project, "frank");
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    act_as_in(&cli_root, "frank");
    let client = common::Client::connect(fixture.root.clone()).await;

    let archived = client
        .call("waymark_archive_work_item", json!({"id": target}))
        .await;
    let mcp = common::envelope(&archived);
    let cli = common::cli_json(&cli_root, &["archive", &target]);
    for field in ["id", "written", "closed_at", "archive_reason"] {
        assert_eq!(
            mcp["data"][field], cli["data"][field],
            "the adapters disagree about `{field}`"
        );
    }
    // The paths name documents in two working trees, so they are compared by
    // their repository-relative form; everything else is compared byte for byte.
    for field in ["path", "removals", "affected", "untouched", "item"] {
        assert!(
            common::same_relative_location(&mcp["data"][field], &cli["data"][field]),
            "the adapters disagree about `{field}`:\nMCP {}\nCLI {}",
            mcp["data"][field],
            cli["data"][field]
        );
    }
    // Without consent the archive wrote the target and named every inbound
    // document, its own included, as untouched.
    assert_eq!(mcp["data"]["written"], true);
    assert_eq!(mcp["data"]["removals"], json!([]));
    assert_eq!(mcp["data"]["affected"], json!([]));
    let untouched: Vec<(&str, &serde_json::Value)> = mcp["data"]["untouched"]
        .as_array()
        .expect("untouched is a list")
        .iter()
        .map(|entry| (entry["id"].as_str().expect("an id"), &entry["relations"]))
        .collect();
    let dependencies = json!(["dependencies"]);
    let soft_links = json!(["soft_links"]);
    let mut expected = vec![
        (dependent.as_str(), &dependencies),
        (linked.as_str(), &soft_links),
    ];
    expected.sort_by_key(|(id, _)| id.to_string());
    assert_eq!(untouched, expected);
    let mcp_warnings: Vec<_> = mcp["_meta"]["warnings"]
        .as_array()
        .expect("warnings is a list")
        .iter()
        .map(warning_shape)
        .collect();
    let cli_warnings: Vec<_> = cli["_meta"]["warnings"]
        .as_array()
        .expect("warnings is a list")
        .iter()
        .map(warning_shape)
        .collect();
    assert_eq!(
        mcp_warnings.len(),
        1,
        "the target belongs to someone else, so the archive warns for it"
    );
    assert_eq!(mcp_warnings[0].0, "OWNER_MISMATCH");
    assert_eq!(mcp_warnings, cli_warnings);

    client.shutdown().await;
}

/// One refusal, driven through both adapters, comes back the same way: same
/// error document, same exit class.
#[tokio::test]
async fn both_adapters_refuse_a_cross_item_write_the_same_way() {
    let fixture = common::fixture(&["one", "two"]);
    let mine = fixture.ids[0].clone();
    let theirs = fixture.ids[1].clone();
    waymark_core::ops::owner::transfer_owner(
        &fixture.project(),
        &theirs,
        "alice",
        &common::options(),
    )
    .expect("the partner belongs to someone else");
    act_as(&fixture.project(), "frank");
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    act_as_in(&cli_root, "frank");
    common::warm_lock(&fixture.root);
    common::warm_lock(&cli_root);
    let before = common::normalized_snapshot(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;

    // Consent is the first refusal: it is the only one the caller fixes alone.
    let consent = WaymarkError::cascade_consent_required(
        "link add",
        &WorkItemId::parse(&mine).expect("a stable ID"),
        &WorkItemId::parse(&theirs).expect("a stable ID"),
        "re-run with --cascade to also record the link there",
    );
    let mcp = common::business_error(
        &client
            .call(
                "waymark_add_soft_link",
                json!({"id": mine, "other": theirs}),
            )
            .await,
    );
    let (exit, cli) = common::try_cli_json_code(&cli_root, &["link", "add", &mine, &theirs]);
    let cli = common::cli_business_error(&cli.expect_err("the CLI refuses as well"));
    assert_eq!(mcp, cli, "the consent refusal differs between adapters");
    assert_eq!(mcp, common::shared_failure(&consent));
    assert_eq!(mcp["code"], json!("CASCADE_CONSENT_REQUIRED"));
    assert_eq!(mcp["details"]["option"], json!("cascade"));
    assert_eq!(consent.exit_class(), Some(ExitClass::Conflict));
    assert_eq!(exit, 4, "the registry's conflict class, not a usage error");

    // With consent, ownership is the answer, and re-running cannot change it.
    let owned = WaymarkError::owned_by_other(
        &WorkItemId::parse(&theirs).expect("a stable ID"),
        "alice",
        "link add",
    );
    let mcp = common::business_error(
        &client
            .call(
                "waymark_add_soft_link",
                json!({"id": mine, "other": theirs, "cascade": true}),
            )
            .await,
    );
    let (exit, cli) =
        common::try_cli_json_code(&cli_root, &["link", "add", &mine, &theirs, "--cascade"]);
    let cli = common::cli_business_error(&cli.expect_err("the CLI refuses as well"));
    assert_eq!(mcp, cli, "the ownership refusal differs between adapters");
    assert_eq!(mcp, common::shared_failure(&owned));
    assert_eq!(mcp["code"], json!("OWNED_BY_OTHER"));
    assert_eq!(mcp["retryable"], json!(false));
    assert_eq!(owned.exit_class(), Some(ExitClass::Conflict));
    assert_eq!(exit, 4, "the registry's conflict class, not a usage error");

    assert_eq!(
        common::normalized_snapshot(&fixture.root),
        before,
        "a refusal writes neither document through either adapter"
    );
    assert_eq!(
        common::normalized_snapshot(&cli_root),
        before,
        "a refusal writes neither document through either adapter"
    );

    client.shutdown().await;
}

#[test]
fn every_deliberate_exception_is_documented_in_the_decisions_record() {
    let decisions = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("docs")
            .join("implementation-decisions.md"),
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
