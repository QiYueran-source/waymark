//! MCP and CLI write the same bytes.
//!
//! Each test drives the same domain operation through both adapters against
//! equivalent copies of one project, then compares the whole on-disk result.
//! Operation-clock values (`created_at`, `updated_at`, `closed_at`,
//! `archived_at`) are normalized because both adapters use system time; every
//! other byte, including the canonical field order, must be identical.

mod common;

use std::collections::BTreeMap;

use camino::Utf8Path;
use common::{act_as, act_as_in};
use serde_json::json;
use waymark_core::config::FieldType;
use waymark_core::model::FieldKey;
use waymark_core::ops::{config, edit, relationship};
use waymark_core::FieldValue;
use waymark_core::WaymarkError;
use waymark_core::WorkItemId;

/// Declare an optional custom field before the project is copied.
fn add_field(fixture: &common::Fixture, key: &str, ty: FieldType) {
    config::add_field(
        &fixture.project(),
        &FieldKey::parse(key).unwrap(),
        ty,
        false,
        None,
        &common::options(),
    )
    .expect("the field is declared");
}

/// Declare a category before the project is copied.
fn add_category(fixture: &common::Fixture, key: &str, path: &str) {
    config::add_category(
        &fixture.project(),
        &waymark_core::CategoryKey::parse(key).unwrap(),
        path,
        None,
        &common::options(),
    )
    .expect("the category is declared");
}

#[tokio::test]
async fn create_matches_the_cli() {
    let fixture = common::fixture(&["one"]);
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_create_work_item",
            json!({
                "title": "New item",
                "category": "task",
                "owner": "frank",
                "body": "the body\n"
            }),
        )
        .await;
    let mcp = common::envelope(&result);
    let cli = common::cli_json(
        &cli_root,
        &[
            "create",
            "--title",
            "New item",
            "--category",
            "task",
            "--owner",
            "frank",
            "--body",
            "the body\n",
        ],
    );

    let mcp_path = camino::Utf8Path::new(mcp["data"]["path"].as_str().unwrap());
    let cli_path = camino::Utf8Path::new(cli["data"]["path"].as_str().unwrap());
    assert_eq!(
        common::normalize_identity(&common::read(mcp_path)),
        common::normalize_identity(&common::read(cli_path)),
        "the created documents differ only in minted identity and clock"
    );
    assert_eq!(mcp["data"]["item"]["title"], cli["data"]["item"]["title"]);
    assert_eq!(
        mcp["data"]["item"]["category"],
        cli["data"]["item"]["category"]
    );
    assert_eq!(mcp["data"]["item"]["owner"], cli["data"]["item"]["owner"]);
    assert_eq!(mcp["data"]["item"]["body"], cli["data"]["item"]["body"]);
    assert_eq!(mcp["data"]["item"]["status"], cli["data"]["item"]["status"]);
    assert_eq!(
        mcp["data"]["item"]["custom_fields"],
        cli["data"]["item"]["custom_fields"]
    );

    client.shutdown().await;
}

/// A whole-tree snapshot normalized for a freshly minted ID.
///
/// The two adapters mint different stable IDs, so the new document's own `id`
/// line, its file name, and every partner's symmetric reference to it differ.
/// Replacing that one literal with a placeholder — after normalizing the
/// operation clock — leaves every other byte, including field order, comparable.
fn normalized_created_tree(root: &Utf8Path, created: &str) -> BTreeMap<String, Vec<u8>> {
    common::snapshot(root)
        .into_iter()
        .map(|(path, bytes)| {
            let path = path.replace(created, "<new-id>");
            let text = String::from_utf8_lossy(&bytes);
            let normalized = common::normalize_clocks(&text).replace(created, "<new-id>");
            (path, normalized.into_bytes())
        })
        .collect()
}

#[tokio::test]
async fn composite_create_matches_the_cli() {
    composite_create_matches_the_cli_with(false).await;
}

#[tokio::test]
async fn cascading_composite_create_matches_the_cli() {
    composite_create_matches_the_cli_with(true).await;
}

/// The IDs in a creation's `untouched` list, in order.
fn untouched_ids(envelope: &serde_json::Value) -> Vec<String> {
    envelope["data"]["untouched"]
        .as_array()
        .expect("untouched is a list")
        .iter()
        .map(|partner| partner["id"].as_str().expect("a partner id").to_owned())
        .collect()
}

/// One composite create, with or without consent, through both adapters.
async fn composite_create_matches_the_cli_with(cascade: bool) {
    let fixture = common::fixture(&["one", "two"]);
    add_field(&fixture, "note", FieldType::String);
    add_field(&fixture, "points", FieldType::Integer);
    let parent = fixture.ids[0].clone();
    let linked = fixture.ids[1].clone();
    // The partners are frank's; the operator is recorded the way a person
    // records one, so `cascade` can rewrite them in both adapters.
    waymark_core::ops::local_config::set_scalar(
        &fixture.project(),
        waymark_core::ops::config::Scalar::Operator,
        "frank",
        &common::options(),
    )
    .expect("the operator is recorded");
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_create_work_item",
            json!({
                "title": "Composite item",
                "category": "task",
                "owner": "frank",
                "creator": "alice",
                "body": "the body\n",
                "custom_fields": {"note": "hello", "points": 7},
                "parent": parent,
                "dependencies": [parent],
                "soft_links": [linked],
                "cascade": cascade
            }),
        )
        .await;
    let mcp = common::envelope(&result);
    assert_eq!(mcp["ok"], true, "{mcp}");
    let mcp_id = mcp["data"]["id"].as_str().expect("the MCP id").to_owned();

    let mut arguments = vec![
        "create",
        "--title",
        "Composite item",
        "--category",
        "task",
        "--owner",
        "frank",
        "--creator",
        "alice",
        "--body",
        "the body\n",
        "--field",
        "note=hello",
        "--field",
        "points=7",
        "--parent",
        &parent,
        "--dependency",
        &parent,
        "--link",
        &linked,
    ];
    if cascade {
        arguments.push("--cascade");
    }
    let cli = common::cli_json(&cli_root, &arguments);
    let cli_id = cli["data"]["id"].as_str().expect("the CLI id").to_owned();

    for field in [
        "title",
        "category",
        "owner",
        "creator",
        "body",
        "status",
        "custom_fields",
        "parent",
        "dependencies",
        "soft_links",
    ] {
        assert_eq!(
            mcp["data"]["item"][field], cli["data"]["item"][field],
            "the adapters disagree about `{field}`"
        );
    }
    let partners = usize::from(cascade);
    assert_eq!(mcp["data"]["partners"].as_array().unwrap().len(), partners);
    assert_eq!(cli["data"]["partners"].as_array().unwrap().len(), partners);
    assert_eq!(
        mcp["data"]["fingerprints"].as_array().unwrap().len(),
        partners + 1
    );
    assert_eq!(
        cli["data"]["fingerprints"].as_array().unwrap().len(),
        partners + 1
    );
    assert_eq!(
        untouched_ids(&mcp),
        untouched_ids(&cli),
        "the adapters disagree about the partners they left alone"
    );
    let expected_untouched = if cascade {
        Vec::new()
    } else {
        vec![linked.clone()]
    };
    assert_eq!(untouched_ids(&mcp), expected_untouched);

    assert_eq!(
        normalized_created_tree(&fixture.root, &mcp_id),
        normalized_created_tree(&cli_root, &cli_id),
        "composite creation must write the same tree through both adapters (cascade={cascade})"
    );

    client.shutdown().await;
}

/// One composite create whose partner belongs to someone else, with or without
/// consent: neither adapter may rewrite that partner, and both must name it as
/// left alone — with identical bytes and identical `untouched` lists.
async fn foreign_partner_create_matches_the_cli_with(cascade: bool) {
    let fixture = common::fixture(&["one"]);
    let linked = fixture.ids[0].clone();
    waymark_core::ops::owner::transfer_owner(
        &fixture.project(),
        &linked,
        "alice",
        &common::options(),
    )
    .expect("the partner belongs to someone else");
    act_as(&fixture.project(), "frank");
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    act_as_in(&cli_root, "frank");
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_create_work_item",
            json!({
                "title": "Composite item",
                "category": "task",
                "owner": "frank",
                "body": "the body\n",
                "soft_links": [linked],
                "cascade": cascade
            }),
        )
        .await;
    let mcp = common::envelope(&result);
    assert_eq!(mcp["ok"], true, "{mcp}");
    let mcp_id = mcp["data"]["id"].as_str().expect("the MCP id").to_owned();

    let mut arguments = vec![
        "create",
        "--title",
        "Composite item",
        "--category",
        "task",
        "--owner",
        "frank",
        "--body",
        "the body\n",
        "--link",
        &linked,
    ];
    if cascade {
        arguments.push("--cascade");
    }
    let cli = common::cli_json(&cli_root, &arguments);
    let cli_id = cli["data"]["id"].as_str().expect("the CLI id").to_owned();

    // The partner is not the operator's, so consent changes nothing: the link
    // is recorded on the new document only and the partner is named.
    assert_eq!(mcp["data"]["partners"], json!([]));
    assert_eq!(cli["data"]["partners"], json!([]));
    assert_eq!(
        untouched_ids(&mcp),
        untouched_ids(&cli),
        "the adapters disagree about the partner they left alone"
    );
    assert_eq!(untouched_ids(&mcp), vec![linked.clone()]);
    assert_eq!(
        mcp["data"]["fingerprints"].as_array().unwrap().len(),
        1,
        "only the new document was written"
    );
    assert_eq!(
        mcp["data"]["item"]["soft_links"],
        cli["data"]["item"]["soft_links"]
    );
    assert_eq!(
        mcp["_meta"]["warnings"].as_array().map(Vec::len),
        cli["_meta"]["warnings"].as_array().map(Vec::len)
    );

    assert_eq!(
        normalized_created_tree(&fixture.root, &mcp_id),
        normalized_created_tree(&cli_root, &cli_id),
        "a create with a foreign partner must write the same tree through both adapters (cascade={cascade})"
    );

    client.shutdown().await;
}

#[tokio::test]
async fn create_with_a_foreign_partner_matches_the_cli() {
    foreign_partner_create_matches_the_cli_with(false).await;
}

#[tokio::test]
async fn cascading_create_with_a_foreign_partner_matches_the_cli() {
    foreign_partner_create_matches_the_cli_with(true).await;
}

/// One soft-link removal across a foreign pair that arrived one-sided from
/// outside Waymark, with or without consent: both adapters refuse the consented
/// form the same way and both rewrite only the caller's document otherwise.
async fn foreign_link_remove_matches_the_cli_with(cascade: bool) {
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
    // Record both halves by hand: `link add` against a foreign partner is
    // refused, so the pair is created the way a merge or a hand edit leaves it.
    for (from, to) in [(&mine, &theirs), (&theirs, &mine)] {
        let path = common::task_path(&fixture.root, from);
        let text = common::read(&path);
        let fence = text.rfind("\n---").expect("a frontmatter fence");
        let mut seeded = text[..fence].to_owned();
        seeded.push_str(&format!("\nsoft_links:\n  - {to}"));
        seeded.push_str(&text[fence..]);
        std::fs::write(&path, seeded).expect("the document is writable");
    }
    act_as(&fixture.project(), "frank");
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    act_as_in(&cli_root, "frank");
    let before = common::normalized_snapshot(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;

    let mut arguments = vec!["link", "remove", &mine, &theirs];
    let mut payload = json!({"id": mine, "other": theirs});
    if cascade {
        arguments.push("--cascade");
        payload["cascade"] = json!(true);
    }

    if cascade {
        let owned = WaymarkError::owned_by_other(
            &WorkItemId::parse(&theirs).expect("a stable ID"),
            "alice",
            "link remove",
        );
        let mcp = common::business_error(&client.call("waymark_remove_soft_link", payload).await);
        let (exit, cli) = common::try_cli_json_code(&cli_root, &arguments);
        let cli = common::cli_business_error(&cli.expect_err("the CLI refuses as well"));
        assert_eq!(mcp, cli, "the ownership refusal differs between adapters");
        assert_eq!(mcp, common::shared_failure(&owned));
        assert_eq!(mcp["code"], json!("OWNED_BY_OTHER"));
        assert_eq!(exit, 4, "the registry's conflict class");
        assert_eq!(
            common::normalized_snapshot(&fixture.root),
            before,
            "the refused removal wrote neither document"
        );
    } else {
        let mcp = common::envelope(&client.call("waymark_remove_soft_link", payload).await);
        assert_eq!(mcp["ok"], true, "{mcp}");
        let cli = common::cli_json(&cli_root, &arguments);
        assert_eq!(mcp["data"]["soft_links"], json!([]));
        assert_eq!(mcp["data"]["other_item"], json!(null));
        for field in ["id", "changed", "written", "verb", "partner_unsynchronised"] {
            assert_eq!(
                mcp["data"][field], cli["data"][field],
                "the adapters disagree about `{field}`"
            );
        }
        // The two copies live at different roots, so the document is compared
        // by its repository-relative path and every other field exactly.
        let mcp_partner = &mcp["data"]["other_untouched"];
        let cli_partner = &cli["data"]["other_untouched"];
        for field in ["id", "owner", "owned"] {
            assert_eq!(
                mcp_partner[field], cli_partner[field],
                "the adapters disagree about the partner's `{field}`"
            );
        }
        assert!(
            common::same_relative_location(&mcp_partner["path"], &cli_partner["path"]),
            "the adapters name different partner documents: {mcp_partner} vs {cli_partner}"
        );
        assert_eq!(mcp["data"]["other_untouched"]["id"], json!(theirs));
        assert_eq!(mcp["data"]["other_untouched"]["owned"], json!(false));
        assert_eq!(
            mcp["data"]["fingerprints"].as_array().unwrap().len(),
            1,
            "only the caller's document was written"
        );
        assert_eq!(
            common::normalized_snapshot(&fixture.root),
            common::normalized_snapshot(&cli_root),
            "the one-sided removal across a foreign pair diverges from the CLI"
        );
    }

    client.shutdown().await;
}

#[tokio::test]
async fn link_remove_across_a_foreign_pair_matches_the_cli() {
    foreign_link_remove_matches_the_cli_with(false).await;
}

#[tokio::test]
async fn cascading_link_remove_across_a_foreign_pair_matches_the_cli() {
    foreign_link_remove_matches_the_cli_with(true).await;
}

#[tokio::test]
async fn title_body_and_custom_fields_match_the_cli() {
    let fixture = common::fixture(&["one"]);
    add_field(&fixture, "points", FieldType::Integer);
    let id = fixture.ids[0].clone();
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;

    for (tool, arguments) in [
        ("waymark_set_title", json!({"id": id, "title": "Renamed"})),
        (
            "waymark_replace_body",
            json!({"id": id, "body": "new body\n"}),
        ),
        (
            "waymark_update_custom_fields",
            json!({"id": id, "custom_fields": {"set": {"points": 7}}}),
        ),
    ] {
        let result = client.call(tool, arguments).await;
        assert_eq!(
            common::envelope(&result)["ok"],
            true,
            "{tool}: {}",
            common::envelope(&result)
        );
    }

    common::cli_json(&cli_root, &["title", "set", &id, "Renamed"]);
    common::cli_json(&cli_root, &["body", "replace", &id, "--body", "new body\n"]);
    common::cli_json(&cli_root, &["field", "update", &id, "--set", "points=7"]);

    assert_eq!(
        common::normalized_snapshot(&fixture.root),
        common::normalized_snapshot(&cli_root),
        "title/body/field writes diverge from the CLI"
    );

    client.shutdown().await;
}

#[tokio::test]
async fn status_matches_the_cli() {
    let fixture = common::fixture(&["one"]);
    let id = fixture.ids[0].clone();
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call("waymark_set_status", json!({"id": id, "status": "closed"}))
        .await;
    assert_eq!(common::envelope(&result)["ok"], true);
    common::cli_json(&cli_root, &["status", "set", &id, "closed"]);

    assert_eq!(
        common::normalized_snapshot(&fixture.root),
        common::normalized_snapshot(&cli_root),
        "the status write diverges from the CLI"
    );

    client.shutdown().await;
}

#[tokio::test]
async fn parent_matches_the_cli() {
    let fixture = common::fixture(&["one", "two"]);
    let child = fixture.ids[1].clone();
    let parent = fixture.ids[0].clone();
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call("waymark_set_parent", json!({"id": child, "parent": parent}))
        .await;
    assert_eq!(common::envelope(&result)["ok"], true);
    common::cli_json(&cli_root, &["parent", "set", &child, &parent]);

    assert_eq!(
        common::normalized_snapshot(&fixture.root),
        common::normalized_snapshot(&cli_root),
        "the parent write diverges from the CLI"
    );

    client.shutdown().await;
}

#[tokio::test]
async fn dependency_matches_the_cli() {
    let fixture = common::fixture(&["one", "two"]);
    let waiting = fixture.ids[1].clone();
    let blocking = fixture.ids[0].clone();
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_add_dependency",
            json!({"id": waiting, "blocking": blocking}),
        )
        .await;
    assert_eq!(common::envelope(&result)["ok"], true);
    common::cli_json(&cli_root, &["dependency", "add", &waiting, &blocking]);

    assert_eq!(
        common::normalized_snapshot(&fixture.root),
        common::normalized_snapshot(&cli_root),
        "the dependency write diverges from the CLI"
    );

    client.shutdown().await;
}

#[tokio::test]
async fn soft_link_matches_the_cli() {
    // Consent is the only way either adapter writes the partner, and both write
    // the same bytes for it: the plain add records one side and leaves the other
    // alone, the consented add completes the pair, and the removal takes it back
    // from both sides. Each step is compared byte for byte across the adapters.
    let fixture = common::fixture(&["one", "two"]);
    let left = fixture.ids[0].clone();
    let right = fixture.ids[1].clone();
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    // Act as the owner on both sides: the CLI through its local document, the
    // server through the override a tool call carries.
    let project = fixture.project();
    act_as(&project, "frank");
    act_as_in(&cli_root, "frank");
    let client = common::Client::connect(fixture.root.clone()).await;

    let one_sided = client
        .call("waymark_add_soft_link", json!({"id": left, "other": right}))
        .await;
    assert_eq!(
        common::envelope(&one_sided)["data"]["partner_unsynchronised"],
        json!(true),
        "the plain add records one side only: {:?}",
        common::envelope(&one_sided)
    );
    common::cli_json(&cli_root, &["link", "add", &left, &right]);
    assert_eq!(
        common::normalized_snapshot(&fixture.root),
        common::normalized_snapshot(&cli_root),
        "the one-sided add diverges from the CLI"
    );

    let result = client
        .call(
            "waymark_add_soft_link",
            json!({"id": left, "other": right, "cascade": true}),
        )
        .await;
    assert_eq!(
        common::envelope(&result)["ok"],
        true,
        "{:?}",
        common::envelope(&result)
    );
    let cli = common::cli_json(&cli_root, &["link", "add", &left, &right, "--cascade"]);
    assert_eq!(
        common::normalized_snapshot(&fixture.root),
        common::normalized_snapshot(&cli_root),
        "the consented soft-link write diverges from the CLI: {cli}"
    );

    let result = client
        .call(
            "waymark_remove_soft_link",
            json!({"id": left, "other": right, "cascade": true}),
        )
        .await;
    assert_eq!(common::envelope(&result)["ok"], true);
    common::cli_json(&cli_root, &["link", "remove", &left, &right, "--cascade"]);
    assert_eq!(
        common::normalized_snapshot(&fixture.root),
        common::normalized_snapshot(&cli_root),
        "the consented soft-link removal diverges from the CLI"
    );

    // A removal without consent writes one document through either adapter.
    common::cli_json(&cli_root, &["link", "add", &left, &right, "--cascade"]);
    let added = client
        .call(
            "waymark_add_soft_link",
            json!({"id": left, "other": right, "cascade": true}),
        )
        .await;
    assert_eq!(common::envelope(&added)["ok"], true);
    let removed = client
        .call(
            "waymark_remove_soft_link",
            json!({"id": left, "other": right}),
        )
        .await;
    assert_eq!(common::envelope(&removed)["ok"], true);
    common::cli_json(&cli_root, &["link", "remove", &left, &right]);
    assert_eq!(
        common::normalized_snapshot(&fixture.root),
        common::normalized_snapshot(&cli_root),
        "the one-sided removal diverges from the CLI"
    );

    client.shutdown().await;
}

#[tokio::test]
async fn owner_matches_the_cli() {
    let fixture = common::fixture(&["one"]);
    let id = fixture.ids[0].clone();
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_transfer_owner",
            json!({"id": id, "owner": "carol"}),
        )
        .await;
    assert_eq!(common::envelope(&result)["ok"], true);
    common::cli_json(&cli_root, &["owner", "transfer", &id, "carol"]);

    assert_eq!(
        common::normalized_snapshot(&fixture.root),
        common::normalized_snapshot(&cli_root),
        "the owner write diverges from the CLI"
    );

    client.shutdown().await;
}

#[tokio::test]
async fn move_matches_the_cli() {
    let fixture = common::fixture(&["one"]);
    add_category(&fixture, "bug", "work/bugs");
    let id = fixture.ids[0].clone();
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_move_work_item",
            json!({"id": id, "category": "bug"}),
        )
        .await;
    assert_eq!(common::envelope(&result)["ok"], true);
    common::cli_json(&cli_root, &["move", &id, "bug"]);

    assert_eq!(
        common::normalized_snapshot(&fixture.root),
        common::normalized_snapshot(&cli_root),
        "the move diverges from the CLI"
    );

    client.shutdown().await;
}

/// One archive, with or without consent, through both adapters.
///
/// The project carries a document that belongs to someone else, so the consent
/// case distinguishes "cleaned up" from "left and named" rather than both
/// adapters doing the same nothing.
async fn archive_matches_the_cli_with(cascade: bool) {
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
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    // Consent asks whether the inbound documents are the operator's, so both
    // adapters have to act as the same person: the CLI reads the local
    // document, and this is where it is written.
    act_as(&project, "frank");
    act_as_in(&cli_root, "frank");
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_archive_work_item",
            json!({"id": target, "reason": "retired", "cascade": cascade}),
        )
        .await;
    assert_eq!(common::envelope(&result)["ok"], true);
    let mut arguments = vec!["archive", &target, "--reason", "retired"];
    if cascade {
        arguments.push("--cascade");
    }
    common::cli_json(&cli_root, &arguments);

    assert_eq!(
        common::normalized_snapshot(&fixture.root),
        common::normalized_snapshot(&cli_root),
        "the archive diverges from the CLI (cascade={cascade})"
    );

    client.shutdown().await;
}

#[tokio::test]
async fn archive_matches_the_cli() {
    archive_matches_the_cli_with(false).await;
}

#[tokio::test]
async fn cascading_archive_matches_the_cli() {
    archive_matches_the_cli_with(true).await;
}

#[tokio::test]
async fn edit_value_shapes_match_the_cli_field_tokens() {
    let fixture = common::fixture(&["one"]);
    add_field(&fixture, "flag", FieldType::Boolean);
    add_field(
        &fixture,
        "tags",
        FieldType::List(Box::new(FieldType::String)),
    );
    let id = fixture.ids[0].clone();
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;

    let result = client
        .call(
            "waymark_update_custom_fields",
            json!({
                "id": id,
                "custom_fields": {"set": {"flag": true, "tags": ["a", "b"]}}
            }),
        )
        .await;
    assert_eq!(common::envelope(&result)["ok"], true);
    common::cli_json(
        &cli_root,
        &[
            "field",
            "update",
            &id,
            "--set",
            "flag=true",
            "--set",
            "tags=[\"a\",\"b\"]",
        ],
    );

    assert_eq!(
        common::normalized_snapshot(&fixture.root),
        common::normalized_snapshot(&cli_root),
        "structured MCP values and CLI tokens must persist identically"
    );

    client.shutdown().await;
}

#[tokio::test]
async fn config_writes_match_the_cli() {
    let fixture = common::fixture(&["one"]);
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;

    for (tool, arguments) in [
        (
            "waymark_config_set",
            json!({"name": "operator", "value": "frank"}),
        ),
        (
            "waymark_config_set",
            json!({"name": "timezone", "value": "Asia/Shanghai"}),
        ),
        (
            "waymark_config_set",
            json!({"name": "cache.capacity", "value": "64"}),
        ),
        (
            "waymark_config_add_category",
            json!({"key": "bug", "path": "work/bugs", "name": "Bugs"}),
        ),
        (
            "waymark_config_add_category",
            json!({"key": "chore", "path": "work/chore"}),
        ),
        (
            "waymark_config_update_category",
            json!({"key": "bug", "name": "Defects"}),
        ),
        (
            "waymark_config_add_field",
            json!({"key": "points", "type": "integer", "default": "0"}),
        ),
        (
            "waymark_config_update_field",
            json!({"key": "points", "attribute": "default", "value": "5"}),
        ),
        ("waymark_config_remove_category", json!({"key": "chore"})),
    ] {
        let result = client.call(tool, arguments).await;
        assert_eq!(common::envelope(&result)["ok"], true, "{tool}");
    }

    common::cli_json(&cli_root, &["config", "set", "operator", "frank"]);
    common::cli_json(&cli_root, &["config", "set", "timezone", "Asia/Shanghai"]);
    common::cli_json(&cli_root, &["config", "set", "cache.capacity", "64"]);
    common::cli_json(
        &cli_root,
        &[
            "config",
            "add",
            "category",
            "bug",
            "work/bugs",
            "--name",
            "Bugs",
        ],
    );
    common::cli_json(
        &cli_root,
        &["config", "add", "category", "chore", "work/chore"],
    );
    common::cli_json(
        &cli_root,
        &["config", "set", "category", "bug", "--name", "Defects"],
    );
    common::cli_json(
        &cli_root,
        &[
            "config",
            "add",
            "field",
            "points",
            "integer",
            "--default",
            "0",
        ],
    );
    common::cli_json(
        &cli_root,
        &["config", "set", "field", "points", "default", "5"],
    );
    common::cli_json(&cli_root, &["config", "remove", "category", "chore"]);

    assert_eq!(
        common::snapshot(&fixture.root),
        common::snapshot(&cli_root),
        "the configuration writes diverge from the CLI"
    );

    client.shutdown().await;
}

#[tokio::test]
async fn migrations_match_the_cli() {
    // Version migration.
    let fixture = common::fixture(&["one"]);
    let id = fixture.ids[0].clone();
    let path = fixture
        .root
        .join("work")
        .join("task")
        .join(format!("{id}.md"));
    let document = common::read(&path);
    let downgraded = document.replacen(
        &format!("schema: {}\n", waymark_core::model::FORMAT_GENERATION),
        &format!("schema: {}\n", waymark_core::model::FORMAT_GENERATION - 1),
        1,
    );
    std::fs::write(&path, downgraded).expect("the document is writable");
    let config_path = fixture.root.join(".waymark").join("config.toml");
    let config_text = common::read(&config_path);
    let current = fixture.project().config().version.to_string();
    let replaced =
        config_text.replacen(&format!("version = \"{current}\""), "version = \"0.2\"", 1);
    assert_ne!(config_text, replaced);
    std::fs::write(&config_path, replaced).expect("the configuration is writable");
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;
    let result = client
        .call(
            "waymark_migrate_version",
            json!({"target_generation": waymark_core::model::FORMAT_GENERATION}),
        )
        .await;
    assert_eq!(common::envelope(&result)["ok"], true);
    common::cli_json(
        &cli_root,
        &[
            "migrate",
            "version",
            "--to",
            &waymark_core::model::FORMAT_GENERATION.to_string(),
        ],
    );
    assert_eq!(
        common::snapshot(&fixture.root),
        common::snapshot(&cli_root),
        "the version migration diverges from the CLI"
    );
    client.shutdown().await;

    // Schema migration.
    let fixture = common::fixture(&["one"]);
    let id = fixture.ids[0].clone();
    add_field(&fixture, "note", FieldType::String);
    edit::update_custom_fields(
        &fixture.project(),
        &id,
        &[edit::FieldChangeEntry::set(
            "note",
            FieldValue::String("hello".to_owned()),
        )],
        &common::options(),
    )
    .expect("the value is written");
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;
    let result = client
        .call(
            "waymark_migrate_schema",
            json!({"action": {"kind": "remove_field", "key": "note"}}),
        )
        .await;
    assert_eq!(common::envelope(&result)["ok"], true);
    common::cli_json(&cli_root, &["migrate", "schema", "--remove-field", "note"]);
    assert_eq!(
        common::snapshot(&fixture.root),
        common::snapshot(&cli_root),
        "the schema migration diverges from the CLI"
    );
    client.shutdown().await;

    // Storage migration.
    let fixture = common::fixture(&["one"]);
    add_category(&fixture, "bug", "work/bugs");
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;
    let result = client
        .call(
            "waymark_migrate_storage",
            json!({"action": {"kind": "category_key", "from": "task", "to": "todo"}}),
        )
        .await;
    assert_eq!(common::envelope(&result)["ok"], true);
    common::cli_json(
        &cli_root,
        &["migrate", "storage", "--category-key", "task", "todo"],
    );
    assert_eq!(
        common::snapshot(&fixture.root),
        common::snapshot(&cli_root),
        "the storage migration diverges from the CLI"
    );
    client.shutdown().await;
}

/// The read renderers are the shared `waymark_core::view` ones, so both
/// adapters must publish the same `data` block for the same project.
#[tokio::test]
async fn read_renderings_match_the_cli() {
    let fixture = common::fixture(&["one", "two"]);
    let id = fixture.ids[0].clone();
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;

    async fn mcp_data(
        client: &common::Client,
        tool: &str,
        arguments: serde_json::Value,
    ) -> serde_json::Value {
        let result = client.call(tool, arguments).await;
        common::envelope(&result)["data"].clone()
    }

    let cases = [
        (
            "waymark_get_work_item",
            json!({"id": id}),
            vec!["show", id.as_str()],
        ),
        (
            "waymark_list_work_items",
            json!({"limit": 100}),
            vec!["list", "--limit", "100"],
        ),
        ("waymark_config_read", json!({}), vec!["config", "show"]),
        ("waymark_get_tree", json!({}), vec!["tree"]),
        (
            "waymark_get_dependency_tree",
            json!({"id": id}),
            vec!["dep", "tree", id.as_str()],
        ),
        (
            "waymark_search_work_items",
            json!({"text": "one", "limit": 100}),
            vec!["search", "one", "--limit", "100"],
        ),
    ];
    for (tool, arguments, cli_args) in cases {
        let mcp = mcp_data(&client, tool, arguments).await;
        let cli = common::cli_json(&cli_root, &cli_args);
        // The two projects live at different paths; nothing else may differ.
        let normalize = |value: &serde_json::Value, root: &camino::Utf8Path| {
            // A serialized value escapes `\`, so the Windows root appears with
            // doubled separators; both spellings have to be replaced.
            let escaped = root.as_str().replace('\\', "\\\\");
            value
                .to_string()
                .replace(&escaped, "<root>")
                .replace(root.as_str(), "<root>")
        };
        assert_eq!(
            normalize(&mcp, &fixture.root),
            normalize(&cli["data"], &cli_root),
            "{tool} differs from `waymark {cli_args:?}`"
        );
    }

    client.shutdown().await;
}

#[tokio::test]
async fn config_read_matches_the_cli() {
    let fixture = common::fixture(&["one"]);
    let (_cli_dir, cli_root) = common::copy_project(&fixture.root);
    let client = common::Client::connect(fixture.root.clone()).await;

    // A hand-edited local document is a supported path, and it exercises the
    // whole local shape: an operator, a zone, and both cache fields.
    let local = "operator = \"frank\"\ntimezone = \"Asia/Shanghai\"\n\n[cache]\nenabled = false\ncapacity = 64\n";
    std::fs::write(fixture.root.join(".waymark").join("user.toml"), local)
        .expect("the MCP side document");
    std::fs::write(cli_root.join(".waymark").join("user.toml"), local)
        .expect("the CLI side document");

    let mcp = common::envelope(&client.call("waymark_config_read", common::no_args()).await);
    let cli = common::cli_json(&cli_root, &["config", "show"]);
    assert_eq!(
        mcp["data"], cli["data"],
        "the two adapters must report the same documents and the same effective values"
    );
    assert_eq!(
        mcp["data"]["sources"]["operator"], "local_document",
        "the source report is part of the contract, not a rendering detail"
    );
    assert_eq!(
        mcp["data"]["sources"]["cache"]["capacity"],
        "local_document"
    );
    assert_eq!(
        mcp["_meta"]["warnings"], cli["_meta"]["warnings"],
        "the same resolution produces the same warnings"
    );

    client.shutdown().await;
}
