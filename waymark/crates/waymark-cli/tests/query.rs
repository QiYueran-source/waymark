//! `query`, `search`, and `stats` end to end.
//!
//! Each test drives the real binary, because the adapter is what has to agree
//! with the core: the flag vocabulary, the JSON shapes, and the exit codes.

use std::process::{Command, Output};

use camino::Utf8PathBuf;

fn waymark() -> Command {
    Command::new(env!("CARGO_BIN_EXE_waymark"))
}

struct Run {
    output: Output,
}

impl Run {
    fn status(&self) -> i32 {
        self.output
            .status
            .code()
            .expect("the process exited normally")
    }

    fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout).into_owned()
    }

    fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.output.stderr).into_owned()
    }

    fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.stdout()).unwrap_or_else(|error| {
            panic!(
                "stdout is not one JSON value ({error}):\nstdout: {}\nstderr: {}",
                self.stdout(),
                self.stderr()
            )
        })
    }
}

fn run(command: &mut Command) -> Run {
    Run {
        output: command.output().expect("the binary runs"),
    }
}

fn tempdir() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 path");
    (dir, path)
}

/// A project that declares one integer, one string, and one list field.
fn project() -> (tempfile::TempDir, Utf8PathBuf) {
    let (dir, root) = tempdir();
    let result = run(waymark().arg("init").arg("--project").arg(&root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());

    let config_path = root.join(".waymark").join("config.toml");
    let config = std::fs::read_to_string(&config_path).expect("the configuration exists");
    let declarations = "\
[fields.effort]
type = \"integer\"
required = false

[fields.team]
type = \"string\"
required = false

[fields.tags]
type = \"list:string\"
required = false
";
    std::fs::write(&config_path, format!("{config}\n{declarations}"))
        .expect("the configuration is writable");
    (dir, root)
}

fn create_with_body(root: &Utf8PathBuf, title: &str, owner: &str, body: &str) -> String {
    let result = run(waymark()
        .arg("--json")
        .arg("create")
        .arg("--project")
        .arg(root)
        .arg("--title")
        .arg(title)
        .arg("--category")
        .arg("task")
        .arg("--owner")
        .arg(owner)
        .arg("--body")
        .arg(body));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    result.json()["data"]["id"]
        .as_str()
        .expect("the envelope names the new id")
        .to_owned()
}

fn create(root: &Utf8PathBuf, title: &str) -> String {
    create_with_body(root, title, "frank", "")
}

fn set_field(root: &Utf8PathBuf, id: &str, assignment: &str) {
    let result = run(waymark()
        .arg("--json")
        .arg("field")
        .arg("update")
        .arg("--project")
        .arg(root)
        .arg(id)
        .arg("--set")
        .arg(assignment));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
}

fn json_ok(root: &Utf8PathBuf, arguments: &[&str]) -> serde_json::Value {
    let mut command = waymark();
    command.arg("--json").arg("--project").arg(root);
    for argument in arguments {
        command.arg(argument);
    }
    let result = run(&mut command);
    assert_eq!(
        result.status(),
        0,
        "stderr: {}\nstdout: {}",
        result.stderr(),
        result.stdout()
    );
    result.json()
}

fn ids(envelope: &serde_json::Value) -> Vec<String> {
    envelope["data"]["items"]
        .as_array()
        .expect("a list payload")
        .iter()
        .map(|row| {
            row["id"]
                .as_str()
                .expect("every row names its id")
                .to_owned()
        })
        .collect()
}

#[test]
fn query_runs_a_structured_filter_document_from_a_literal() {
    let (_dir, root) = project();
    let low = create(&root, "low");
    let high = create(&root, "high");
    create(&root, "none");
    set_field(&root, &low, "effort=1");
    set_field(&root, &high, "effort=9");

    let envelope = json_ok(
        &root,
        &[
            "query",
            "--filter",
            r#"{"field": [{"key": "effort", "op": "ge", "value": 5}]}"#,
        ],
    );
    assert_eq!(ids(&envelope), vec![high]);
    assert_eq!(envelope["data"]["count"], serde_json::json!(1));
    assert_eq!(
        envelope["_meta"]["source"],
        serde_json::json!("memory_projection")
    );
}

#[test]
fn query_reads_the_filter_from_a_file_and_from_stdin() {
    let (_dir, root) = project();
    let alpha = create(&root, "alpha");
    let beta = create(&root, "beta");
    set_field(&root, &alpha, "team=core");
    set_field(&root, &beta, "team=web");

    let filter_path = root.join("filter.json");
    std::fs::write(
        &filter_path,
        r#"{"field": [{"key": "team", "op": "eq", "value": "web"}]}"#,
    )
    .expect("the filter is writable");
    let from_file = json_ok(&root, &["query", "--filter", &format!("@{}", filter_path)]);
    assert_eq!(ids(&from_file), vec![beta.clone()]);

    // The same document through stdin produces the same answer.
    let mut command = waymark();
    command
        .arg("--json")
        .arg("--project")
        .arg(&root)
        .arg("query")
        .arg("--filter")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().expect("the binary starts");
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().expect("a stdin pipe");
        stdin
            .write_all(br#"{"field": [{"key": "team", "op": "eq", "value": "web"}]}"#)
            .expect("the filter is writable");
    }
    let output = child.wait_with_output().expect("the binary finishes");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON value");
    assert_eq!(ids(&envelope), vec![beta]);
}

#[test]
fn query_rejects_a_malformed_document_as_a_usage_error() {
    let (_dir, root) = project();
    create(&root, "item");

    let result = run(waymark()
        .arg("--json")
        .arg("--project")
        .arg(&root)
        .arg("query")
        .arg("--filter")
        .arg("{not json"));
    assert_eq!(result.status(), 3, "stderr: {}", result.stderr());
    let envelope = result.json();
    assert_eq!(envelope["ok"], serde_json::json!(false));
    assert_eq!(
        envelope["error"]["code"],
        serde_json::json!("VALIDATION_ERROR")
    );
}

#[test]
fn the_list_commands_share_the_filter_sort_and_window_vocabulary() {
    let (_dir, root) = project();
    let blocker = create(&root, "blocker");
    let dependent = create(&root, "dependent");
    let done = create(&root, "done");
    json_ok(&root, &["dependency", "add", &dependent, &blocker]);
    json_ok(&root, &["close", &done]);

    // `--status` means the persisted lifecycle state on all three commands.
    let open_only = json_ok(&root, &["list", "--status", "open"]);
    assert_eq!(ids(&open_only).len(), 2);
    assert!(!ids(&open_only).contains(&done));

    assert_eq!(ids(&json_ok(&root, &["ready"])), vec![blocker.clone()]);
    assert_eq!(ids(&json_ok(&root, &["blocked"])), vec![dependent.clone()]);

    // Ordering and windowing are the same vocabulary everywhere.
    let titled = json_ok(&root, &["list", "--sort", "title", "--desc"]);
    assert_eq!(
        titled["data"]["items"][0]["item"]["title"],
        serde_json::json!("done")
    );
    let windowed = json_ok(&root, &["list", "--limit", "1", "--offset", "1"]);
    assert_eq!(windowed["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(windowed["data"]["total"], serde_json::json!(3));

    // `--where` is the command-line spelling of one field predicate.
    set_field(&root, &blocker, "team=core");
    let filtered = json_ok(&root, &["list", "--where", "team eq core"]);
    assert_eq!(ids(&filtered), vec![blocker]);
    let present = json_ok(&root, &["list", "--where", "team exists"]);
    assert_eq!(present["data"]["count"], serde_json::json!(1));
}

#[test]
fn the_projection_flags_narrow_the_row_without_dropping_the_identity() {
    let (_dir, root) = project();
    let id = create(&root, "titled");

    let compact = json_ok(&root, &["list", "--compact"]);
    let row = &compact["data"]["items"][0];
    assert_eq!(row["id"], serde_json::json!(id));
    assert_eq!(row["item"]["status"], serde_json::json!("open"));
    assert!(row.get("blocked_by").is_none(), "{row}");

    let named = json_ok(&root, &["list", "--fields", "id,title"]);
    let row = &named["data"]["items"][0];
    assert_eq!(row["id"], serde_json::json!(id));
    assert_eq!(row["item"]["title"], serde_json::json!("titled"));
    assert!(row["item"].get("status").is_none(), "{row}");
    assert_eq!(named["data"]["fields"], serde_json::json!(["id", "title"]));
}

#[test]
fn search_puts_title_hits_first_and_never_returns_a_snippet() {
    let (_dir, root) = project();
    let body_only = create_with_body(&root, "alpha", "frank", "the DATABASE is slow");
    let title_hit = create(&root, "database migration");
    create(&root, "unrelated");

    let envelope = json_ok(&root, &["search", "DaTaBaSe"]);
    assert_eq!(ids(&envelope), vec![title_hit, body_only]);
    assert_eq!(
        envelope["data"]["items"][0]["title_hit"],
        serde_json::json!(true)
    );
    assert_eq!(
        envelope["data"]["items"][1]["title_hit"],
        serde_json::json!(false)
    );
    assert_eq!(envelope["data"]["snippets"], serde_json::json!(false));
    assert_eq!(envelope["data"]["count"], serde_json::json!(2));
    // The projected row still carries no body: a snippet is not a body dump.
    assert!(envelope["data"]["items"][1]["item"].get("body").is_none());
}

#[test]
fn search_requires_a_needle_and_accepts_the_filter_flags() {
    let (_dir, root) = project();
    create(&root, "database one");
    let other = create_with_body(&root, "other", "frank", "database two");

    let scoped = json_ok(
        &root,
        &["search", "database", "--status", "open", "--limit", "1"],
    );
    assert_eq!(scoped["data"]["total"], serde_json::json!(2));
    assert_eq!(scoped["data"]["items"].as_array().unwrap().len(), 1);
    assert!(!ids(&scoped).is_empty());
    assert_ne!(ids(&scoped)[0], other);

    let result = run(waymark()
        .arg("--json")
        .arg("--project")
        .arg(&root)
        .arg("search")
        .arg("   "));
    assert_eq!(result.status(), 3, "stderr: {}", result.stderr());
    assert_eq!(
        result.json()["error"]["code"],
        serde_json::json!("VALIDATION_ERROR")
    );
}

#[test]
fn stats_reports_counts_only_and_names_what_it_never_reports() {
    let (_dir, root) = project();
    let blocker = create(&root, "blocker");
    let dependent = create(&root, "dependent");
    let done = create(&root, "done");
    json_ok(&root, &["dependency", "add", &dependent, &blocker]);
    json_ok(&root, &["close", &done]);
    set_field(&root, &blocker, "team=core");
    set_field(&root, &dependent, "team=core");
    set_field(&root, &done, "team=web");

    let envelope = json_ok(&root, &["stats"]);
    let data = &envelope["data"];
    assert_eq!(data["total"], serde_json::json!(3));
    assert_eq!(data["by_status"]["open"], serde_json::json!(2));
    assert_eq!(data["by_status"]["closed"], serde_json::json!(1));
    assert_eq!(data["by_category"]["task"], serde_json::json!(3));
    assert_eq!(data["ready"], serde_json::json!(1));
    assert_eq!(data["blocked"], serde_json::json!(1));
    assert_eq!(data["archive"], serde_json::json!("active"));
    assert_eq!(data["completeness"], serde_json::json!("complete"));
    assert_eq!(data["freshness"], serde_json::json!("fresh"));
    assert_eq!(data["counts_only"], serde_json::json!(true));
    assert!(data.get("history").is_none(), "{data}");
    assert!(data.get("throughput").is_none(), "{data}");
    assert_eq!(
        envelope["_meta"]["completeness"],
        serde_json::json!("complete")
    );

    let with_top = json_ok(&root, &["stats", "--top", "1", "--top-field", "team"]);
    let buckets = with_top["data"]["distribution"]["buckets"]
        .as_array()
        .expect("a distribution");
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0]["value"], serde_json::json!("core"));
    assert_eq!(buckets[0]["count"], serde_json::json!(2));
}

#[test]
fn stats_rejects_a_distribution_over_a_non_string_field() {
    let (_dir, root) = project();
    let id = create(&root, "item");
    set_field(&root, &id, "effort=3");

    let result = run(waymark()
        .arg("--json")
        .arg("--project")
        .arg(&root)
        .arg("stats")
        .arg("--top")
        .arg("3")
        .arg("--top-field")
        .arg("effort"));
    assert_eq!(result.status(), 3, "stderr: {}", result.stderr());
    assert_eq!(
        result.json()["error"]["code"],
        serde_json::json!("VALIDATION_ERROR")
    );
}

#[test]
fn a_human_invocation_writes_the_answer_to_stderr_only() {
    let (_dir, root) = project();
    create(&root, "titled");

    for arguments in [
        vec!["list"],
        vec!["ready"],
        vec!["blocked"],
        vec!["query", "--filter", "{}"],
        vec!["search", "titled"],
        vec!["stats"],
    ] {
        let mut command = waymark();
        command.arg("--project").arg(&root);
        for argument in &arguments {
            command.arg(argument);
        }
        let result = run(&mut command);
        assert_eq!(
            result.status(),
            0,
            "{arguments:?} failed: {}",
            result.stderr()
        );
        assert!(
            result.stdout().is_empty(),
            "{arguments:?} wrote to stdout: {}",
            result.stdout()
        );
        assert!(!result.stderr().is_empty(), "{arguments:?} said nothing");
    }
}

/// `query` takes its filters from the document only. A filter flag is a usage
/// error, not a second, silently discarded filter input.
#[test]
fn query_has_no_filter_flags_to_contradict_the_document() {
    let (_dir, root) = project();
    create(&root, "open item");

    let result = run(waymark()
        .arg("--json")
        .arg("--project")
        .arg(&root)
        .arg("query")
        .arg("--filter")
        .arg("{}")
        .arg("--status")
        .arg("closed"));
    assert_eq!(result.status(), 1, "stderr: {}", result.stderr());
    assert_eq!(
        result.json()["error"]["code"],
        serde_json::json!("USAGE_ERROR")
    );
}

/// The document owns the filters and the archive range, and the rendered
/// metadata reports the range that was actually read.
#[test]
fn query_reads_and_reports_the_document_filter() {
    let (_dir, root) = project();
    let open = create(&root, "open item");
    let archived = create(&root, "archived item");

    let archived_result = run(waymark()
        .arg("--json")
        .arg("--project")
        .arg(&root)
        .arg("archive")
        .arg(&archived));
    assert_eq!(
        archived_result.status(),
        0,
        "stderr: {}",
        archived_result.stderr()
    );

    // A status filter in the document excludes the open item.
    let filtered = run(waymark()
        .arg("--json")
        .arg("--project")
        .arg(&root)
        .arg("query")
        .arg("--filter")
        .arg(r#"{"status":"closed"}"#));
    assert_eq!(filtered.status(), 0, "stderr: {}", filtered.stderr());
    let data = &filtered.json()["data"];
    assert_eq!(data["count"], 0, "{data}");
    assert_eq!(data["archive"], "active", "{data}");

    // The document's archive range is both what is read and what is reported.
    let archived_only = run(waymark()
        .arg("--json")
        .arg("--project")
        .arg(&root)
        .arg("query")
        .arg("--filter")
        .arg(r#"{"archive":"archived"}"#));
    assert_eq!(
        archived_only.status(),
        0,
        "stderr: {}",
        archived_only.stderr()
    );
    let data = &archived_only.json()["data"];
    assert_eq!(data["archive"], "archived", "{data}");
    assert_eq!(data["count"], 1, "{data}");
    assert_eq!(
        data["items"][0]["id"],
        serde_json::json!(archived),
        "{data}"
    );
    assert_ne!(data["items"][0]["id"], serde_json::json!(open), "{data}");

    // An empty document means "no filters" over the active range.
    let unfiltered = run(waymark()
        .arg("--json")
        .arg("--project")
        .arg(&root)
        .arg("query")
        .arg("--filter")
        .arg("{}"));
    assert_eq!(unfiltered.status(), 0, "stderr: {}", unfiltered.stderr());
    let data = &unfiltered.json()["data"];
    assert_eq!(data["archive"], "active", "{data}");
    assert_eq!(data["count"], 1, "{data}");
    assert_eq!(data["items"][0]["id"], serde_json::json!(open), "{data}");
}
