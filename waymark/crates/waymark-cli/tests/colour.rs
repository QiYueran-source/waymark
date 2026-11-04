//! Work-state colour at the command-line seam.
//!
//! These tests drive the real binary through a pipe, so `auto` is exercised in
//! its non-terminal branch; the terminal branch is covered deterministically by
//! the resolver's unit tests in `src/cmd/colour.rs`. The flag matrix, the
//! `NO_COLOR` precedence, and the guarantee that stripping ANSI recovers the
//! exact plain text are all asserted here.

use std::io::Write as _;
use std::process::{Command, Output};

use camino::Utf8PathBuf;

/// The binary under test, with `NO_COLOR` removed so the harness environment
/// cannot change the result of the cases that do not set it themselves.
fn waymark() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_waymark"));
    command.env_remove("NO_COLOR");
    command
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

fn init(root: &Utf8PathBuf) {
    let result = run(waymark().arg("init").arg("--project").arg(root));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
}

/// Create one open item and return its stable ID.
fn create(root: &Utf8PathBuf, title: &str) -> String {
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
        .arg("frank"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    serde_json::from_str::<serde_json::Value>(&result.stdout())
        .expect("a create envelope")
        .get("data")
        .and_then(|data| data.get("id"))
        .and_then(|id| id.as_str())
        .expect("the envelope names the new id")
        .to_owned()
}

fn has_ansi(text: &str) -> bool {
    text.contains('\u{1b}')
}

#[test]
fn auto_through_a_pipe_is_plain_text() {
    let (_dir, root) = tempdir();
    init(&root);
    create(&root, "Ready item");

    let default = run(waymark().arg("list").arg("--project").arg(&root));
    assert_eq!(default.status(), 0, "stderr: {}", default.stderr());
    assert!(!has_ansi(&default.stderr()), "{}", default.stderr());
    assert!(default.stderr().contains("ready"), "{}", default.stderr());

    let explicit = run(waymark()
        .arg("list")
        .arg("--project")
        .arg(&root)
        .arg("--color=auto"));
    assert_eq!(explicit.status(), 0, "stderr: {}", explicit.stderr());
    assert_eq!(explicit.stdout(), default.stdout());
    assert_eq!(explicit.stderr(), default.stderr());
}

#[test]
fn always_forces_ansi_through_a_pipe() {
    let (_dir, root) = tempdir();
    init(&root);
    create(&root, "Ready item");

    let plain = run(waymark().arg("list").arg("--project").arg(&root));
    let coloured = run(waymark()
        .arg("list")
        .arg("--project")
        .arg(&root)
        .arg("--color=always"));
    assert_eq!(coloured.status(), 0, "stderr: {}", coloured.stderr());
    assert!(has_ansi(&coloured.stderr()), "{}", coloured.stderr());
    assert_ne!(coloured.stderr(), plain.stderr());
}

#[test]
fn never_is_byte_identical_to_the_plain_output() {
    let (_dir, root) = tempdir();
    init(&root);
    create(&root, "Ready item");

    let plain = run(waymark().arg("list").arg("--project").arg(&root));
    let never = run(waymark()
        .arg("list")
        .arg("--project")
        .arg(&root)
        .arg("--color=never"));
    assert_eq!(never.status(), 0, "stderr: {}", never.stderr());
    assert_eq!(never.stdout(), plain.stdout());
    assert_eq!(never.stderr(), plain.stderr());
    assert!(!has_ansi(&never.stderr()), "{}", never.stderr());
}

#[test]
fn no_color_disables_auto_but_an_explicit_always_wins() {
    let (_dir, root) = tempdir();
    init(&root);
    create(&root, "Ready item");

    let plain = run(waymark().arg("list").arg("--project").arg(&root));

    let auto = run(waymark()
        .env("NO_COLOR", "1")
        .arg("list")
        .arg("--project")
        .arg(&root)
        .arg("--color=auto"));
    assert_eq!(auto.status(), 0, "stderr: {}", auto.stderr());
    assert_eq!(auto.stderr(), plain.stderr());
    assert!(!has_ansi(&auto.stderr()), "{}", auto.stderr());

    let never = run(waymark()
        .env("NO_COLOR", "1")
        .arg("list")
        .arg("--project")
        .arg(&root)
        .arg("--color=never"));
    assert_eq!(never.stderr(), plain.stderr());

    let forced = run(waymark()
        .env("NO_COLOR", "1")
        .arg("list")
        .arg("--project")
        .arg(&root)
        .arg("--color=always"));
    assert_eq!(forced.status(), 0, "stderr: {}", forced.stderr());
    assert!(has_ansi(&forced.stderr()), "{}", forced.stderr());
}

/// Every row of a listing, addressed by the stable ID it starts with.
fn line_containing<'a>(text: &'a str, needle: &str) -> &'a str {
    text.lines()
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("no line contains {needle:?}:\n{text}"))
}

/// Remove SGR sequences so the coloured text can be compared with the plain
/// text the layout promised.
fn strip_ansi(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            plain.push(character);
            continue;
        }
        if characters.next() != Some('[') {
            continue;
        }
        for next in characters.by_ref() {
            if ('@'..='~').contains(&next) {
                break;
            }
        }
    }
    plain
}

/// One item in each display state, plus the blocked-by edge that proves a
/// blocked row keeps its marker.
struct EveryState {
    _dir: tempfile::TempDir,
    root: Utf8PathBuf,
    ready: String,
    running: String,
    blocker: String,
    blocked: String,
    done: String,
}

fn fixture_with_every_state() -> EveryState {
    let (dir, root) = tempdir();
    init(&root);
    let ready = create(&root, "Ready item");
    let running = create(&root, "Running item");
    let started = run(waymark()
        .arg("start")
        .arg(&running)
        .arg("--project")
        .arg(&root));
    assert_eq!(started.status(), 0, "stderr: {}", started.stderr());
    let blocker = create(&root, "Blocker item");
    let blocked = create(&root, "Blocked item");
    let dependency = run(waymark()
        .arg("dependency")
        .arg("add")
        .arg(&blocked)
        .arg(&blocker)
        .arg("--project")
        .arg(&root));
    assert_eq!(dependency.status(), 0, "stderr: {}", dependency.stderr());
    let done = create(&root, "Done item");
    let closed = run(waymark()
        .arg("close")
        .arg(&done)
        .arg("--project")
        .arg(&root));
    assert_eq!(closed.status(), 0, "stderr: {}", closed.stderr());
    EveryState {
        _dir: dir,
        root,
        ready,
        running,
        blocker,
        blocked,
        done,
    }
}

#[test]
fn each_state_gets_its_semantic_ansi_colour() {
    let fixture = fixture_with_every_state();
    let result = run(waymark()
        .arg("list")
        .arg("--project")
        .arg(&fixture.root)
        .arg("--color=always"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    let stderr = result.stderr();

    assert!(
        line_containing(&stderr, &fixture.ready).contains("\u{1b}[32mready   \u{1b}[0m"),
        "ready should be green:\n{stderr}"
    );
    assert!(
        line_containing(&stderr, &fixture.running).contains("\u{1b}[34mready   \u{1b}[0m"),
        "in-progress should be blue:\n{stderr}"
    );
    assert!(
        line_containing(&stderr, &fixture.blocked).contains("\u{1b}[31mblocked \u{1b}[0m"),
        "blocked should be red:\n{stderr}"
    );
    assert!(
        line_containing(&stderr, &fixture.done).contains("\u{1b}[90m-       \u{1b}[0m"),
        "closed should be grey:\n{stderr}"
    );

    // The colour is never the only carrier: the words stay, including the
    // blocked marker line, and that line is not itself coloured.
    assert!(
        stderr.contains(&format!("    blocked by {}", fixture.blocker)),
        "the blocked marker line should be plain:\n{stderr}"
    );
}

#[test]
fn stripping_ansi_recovers_the_plain_layout_byte_for_byte() {
    let fixture = fixture_with_every_state();
    let plain = run(waymark()
        .arg("list")
        .arg("--project")
        .arg(&fixture.root)
        .arg("--color=never"));
    let coloured = run(waymark()
        .arg("list")
        .arg("--project")
        .arg(&fixture.root)
        .arg("--color=always"));
    assert_eq!(plain.status(), 0, "stderr: {}", plain.stderr());
    assert_eq!(coloured.status(), 0, "stderr: {}", coloured.stderr());
    assert!(has_ansi(&coloured.stderr()), "{}", coloured.stderr());
    assert_eq!(strip_ansi(&coloured.stderr()), plain.stderr());
    assert_eq!(coloured.stdout(), plain.stdout());
}

#[test]
fn show_human_output_colours_only_the_derived_state_line() {
    let fixture = fixture_with_every_state();
    let plain = run(waymark()
        .arg("show")
        .arg(&fixture.blocked)
        .arg("--project")
        .arg(&fixture.root)
        .arg("--color=never"));
    let coloured = run(waymark()
        .arg("show")
        .arg(&fixture.blocked)
        .arg("--project")
        .arg(&fixture.root)
        .arg("--color=always"));
    assert_eq!(coloured.status(), 0, "stderr: {}", coloured.stderr());
    assert!(
        coloured
            .stderr()
            .contains("  state       \u{1b}[31mblocked\u{1b}[0m"),
        "{}",
        coloured.stderr()
    );
    assert!(
        coloured
            .stderr()
            .contains(&format!("  blocked by  {}", fixture.blocker)),
        "the blocked marker stays plain:\n{}",
        coloured.stderr()
    );
    assert_eq!(strip_ansi(&coloured.stderr()), plain.stderr());
    assert_eq!(coloured.stdout(), plain.stdout());
}

#[test]
fn query_shares_the_listing_colour_and_stays_plain_when_stripped() {
    let fixture = fixture_with_every_state();
    let plain = run(waymark()
        .arg("query")
        .arg("--filter")
        .arg("{}")
        .arg("--project")
        .arg(&fixture.root)
        .arg("--color=never"));
    let coloured = run(waymark()
        .arg("query")
        .arg("--filter")
        .arg("{}")
        .arg("--project")
        .arg(&fixture.root)
        .arg("--color=always"));
    assert_eq!(coloured.status(), 0, "stderr: {}", coloured.stderr());
    assert!(has_ansi(&coloured.stderr()), "{}", coloured.stderr());
    assert_eq!(strip_ansi(&coloured.stderr()), plain.stderr());
    assert_eq!(coloured.stdout(), plain.stdout());
}

#[test]
fn json_envelopes_stay_uncoloured() {
    let fixture = fixture_with_every_state();
    let result = run(waymark()
        .arg("--json")
        .arg("list")
        .arg("--project")
        .arg(&fixture.root)
        .arg("--color=always"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    assert!(!has_ansi(&result.stdout()), "{}", result.stdout());
    assert!(
        serde_json::from_str::<serde_json::Value>(&result.stdout()).is_ok(),
        "{}",
        result.stdout()
    );
    assert!(result.stderr().is_empty(), "{}", result.stderr());
}

#[test]
fn show_raw_stays_byte_identical_and_uncoloured() {
    let fixture = fixture_with_every_state();
    let plain = run(waymark()
        .arg("show")
        .arg(&fixture.ready)
        .arg("--raw")
        .arg("--project")
        .arg(&fixture.root)
        .arg("--color=never"));
    let coloured = run(waymark()
        .arg("show")
        .arg(&fixture.ready)
        .arg("--raw")
        .arg("--project")
        .arg(&fixture.root)
        .arg("--color=always"));
    assert_eq!(coloured.status(), 0, "stderr: {}", coloured.stderr());
    assert!(!coloured.stdout().is_empty());
    assert!(!has_ansi(&coloured.stdout()), "{}", coloured.stdout());
    assert_eq!(coloured.stdout(), plain.stdout());
    assert!(coloured.stderr().is_empty(), "{}", coloured.stderr());
}

fn git_available() -> bool {
    waymark_core::git::find_executable().is_some()
}

fn git(root: &Utf8PathBuf, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "user.name=Waymark Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .expect("the git binary runs");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_raw_git_diff_patch_stays_uncoloured() {
    if !git_available() {
        eprintln!("skipping: no `git` executable on PATH");
        return;
    }
    let (_dir, root) = tempdir();
    init(&root);
    let id = create(&root, "Fix the thing");
    git(&root, &["init", "--quiet"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "--quiet", "-m", "add the item"]);

    let shown = run(waymark()
        .arg("--json")
        .arg("show")
        .arg(&id)
        .arg("--project")
        .arg(&root));
    assert_eq!(shown.status(), 0, "stderr: {}", shown.stderr());
    let path = serde_json::from_str::<serde_json::Value>(&shown.stdout())
        .expect("a show envelope")
        .get("data")
        .and_then(|data| data.get("path"))
        .and_then(|path| path.as_str())
        .expect("the show envelope names the document")
        .to_owned();
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("open the work-item document")
        .write_all(b"\nA changed line.\n")
        .expect("append to the work-item document");
    let result = run(waymark()
        .arg("git")
        .arg("diff")
        .arg("--project")
        .arg(&root)
        .arg("--color=always"));
    assert_eq!(result.status(), 0, "stderr: {}", result.stderr());
    assert!(
        result.stdout().contains("diff --git"),
        "expected a patch:\n{}",
        result.stdout()
    );
    assert!(!has_ansi(&result.stdout()), "{}", result.stdout());
}
