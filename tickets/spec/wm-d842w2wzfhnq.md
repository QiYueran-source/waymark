---
schema: 1
id: wm-d842w2wzfhnq
title: macOS native verification plan
category: spec
status: open
owner: frank
creator: frank
created_at: 2026-11-05T15:09:24Z
updated_at: 2026-11-05T15:09:24Z
parent: wm-srxj8ztv1zfz
---
## Parent

[Waymark functional specification](wm-srxj8ztv1zfz)

## What this is

A native verification plan for a macOS machine. It is a runbook for an agent or a person who clones this repository on macOS and reports what the product actually does there; it is **not** an implementation ticket.

CI already runs the full gate on `macos-latest` (Apple Silicon). A native machine adds what the hosted runner does not: a case-sensitive volume option, `/tmp` being a symlink to `/private/tmp`, Apple Silicon vs Intel, POSIX permissions and `O_NOFOLLOW`, the real `open` command, Gatekeeper and quarantine attributes on a packaged binary, and the system time zone rather than the runner's UTC.

## Rules for the operator

- Do not modify source code, tests, or configuration in this repository. The deliverable is evidence, not a fix.
- Every probe is **PASS**, **FAIL**, **ENV-LIMITED** (the machine cannot perform the operation), or **KNOWN-OPEN** (a known gap owned by another effort). Say which, every time.
- Capture the exact command, its exit code, and enough output to reproduce.
- If a probe fails, keep going.

## Environment to record first

| Fact | Command |
| --- | --- |
| OS version | `sw_vers` |
| Architecture | `uname -m` (arm64 vs x86_64) |
| Filesystem and case sensitivity | `mount \| grep " on / "`, and `diskutil info / \| grep -i "File System"` |
| Volume case behaviour | `touch /tmp/CaseTest && ls /tmp/casetest` (both names resolving means case-insensitive) |
| Rust | `rustc --version`, `cargo --version`, `rustup toolchain list` |
| Node | `node --version`, `npm --version` |
| Git | `git --version`, `git config --get core.autocrlf`, `git config --get core.ignorecase` |
| System time zone | `sudo systemsetup -gettimezone` (or `readlink /etc/localtime`), plus `echo "$TZ"` |
| Shell | `zsh --version`, `bash --version` |

Prerequisites: Xcode command line tools (`xcode-select -p`), Rust stable **and** 1.85.0, Node 22, and network for the first `cargo fetch` / `npm ci`. If `~/.cargo` is not writable, set `CARGO_HOME` and `CARGO_TARGET_DIR` to a writable location and say so.

## Baseline gate (same as CI; all must pass)

Clone, check out `<SHA>`, confirm `git status --porcelain` is empty. From `waymark/`:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features --no-fail-fast
cargo test --workspace --no-fail-fast
cargo tree -p waymark-cli -e features | grep waymark-web    # must print nothing
```

MSRV:

```bash
rustup toolchain install 1.85.0
cargo +1.85.0 build --workspace --locked
cargo +1.85.0 test --workspace --locked
```

Frontend and embedding:

```bash
cd waymark/crates/waymark-web/ui
npm ci && npm run check && npm run build && npm test
cd ../../..
cargo run -p waymark-web --bin waymark-web-manifest
```

Build-variant smoke (same assertions as CI):

```bash
manifest="$PWD/Cargo.toml"      # run this from waymark/
project="$(mktemp -d)"; cd "$project"
waymark init
waymark --json serve            # expect exit 3 and WEB_NOT_INCLUDED
cargo run -q --manifest-path "$manifest" -p waymark-cli --features web --bin waymark -- version
# expect the version line to report the web capability
```

Record each command's exit code and wall time.

## Native probes

### M1 — case-sensitive volume, when available

If the machine has (or can create) an APFS case-sensitive volume, repeat the duplicate-case probe there:

```bash
id=$(waymark create --title "case" --category task --owner frank --json | jq -r .data.id)
cp "work/task/$id.md" "work/task/$(echo "$id" | tr 'a-z' 'A-Z').md"
waymark --json validate
```

Expected: on a case-sensitive volume, two distinct documents and a case-variant finding. On the default case-insensitive volume, the copy overwrites the original; record which happened and where.

### M2 — `/tmp` is a symlink

```bash
cd "$(mktemp -d)"; waymark init
waymark create --title "tmp" --category task --owner frank
waymark show <id> --json
waymark git --help >/dev/null 2>&1 || true
```

Expected: the project root reported by `waymark show`/`config show` is usable, and paths may appear as `/private/tmp/...`. Record whether any path is reported inconsistently (`/tmp` vs `/private/tmp`) and whether that breaks a follow-up command.

### M3 — both architectures

On Apple Silicon record the architecture; if an Intel machine (or Rosetta shell) is available, repeat the baseline gate there and record both results. `rustup target list --installed` says what is available.

### M4 — POSIX permissions

```bash
chmod 0444 .waymark/.gitignore        # read-only ignore file
waymark create --title "perm" --category task --owner frank   # the lock entry exists already
waymark config set operator frank     # the local write must refuse the unsafe ignore file
chmod 0644 .waymark/.gitignore
```

Expected: the local write fails with a clear refusal (an unsafe ignore file); a work-item write is unaffected. Record the exit codes and messages.

### M5 — symlinked lock file is not followed

```bash
mv .waymark/write.lock /tmp/real.lock
ln -s /tmp/real.lock .waymark/write.lock
waymark create --title "link" --category task --owner frank
```

Expected: refused with `LOCK_FAILED` (the lock path is a symbolic link and will not be followed) and the target untouched. Restore the file afterwards.

### M6 — symlinked category directory

```bash
ln -s /tmp/real-category work/linked
# declare [categories.linked] path = "work/linked" by hand
waymark --json validate
```

Expected: a symbolic-link refusal finding naming the category path.

### M7 — `serve --open` uses the real opener

```bash
waymark serve --port 0 --open
```

Expected: the default browser opens, the tree renders, and the detail panel's **Created** line shows the system zone plus an offset (compare with `waymark config show`). Quit with Ctrl-C.

### M8 — system time zone

```bash
TZ=Asia/Shanghai waymark config show --json   # read data.effective.timezone
TZ=Europe/Berlin  waymark config show --json   # read data.effective.timezone
```

Expected: with no local document, the effective zone follows the machine (and `TZ` if the platform honours it). Record both values and whether the display timestamps in M7 changed with them.

### M9 — CRLF hand edit

Rewrite one document with CRLF endings (e.g. `perl -pi -e 's/\n/\r\n/' work/task/<id>.md`) and run `waymark show <id> --json` plus `waymark --json validate`.

Expected: the document still parses and its field values are unchanged; record whether the next Waymark write normalizes it back to LF.

### M10 — packaged binary and Gatekeeper (only if a release artifact is supplied)

```bash
xattr -l waymark            # quarantine attribute, if any
spctl -a -vv waymark        # Gatekeeper verdict
./waymark version
./waymark-mcp --help
```

Expected: the binary runs from a terminal; record the quarantine and Gatekeeper verdict verbatim.

## Evidence table (fill this in)

| Probe | Command | Exit | Expected | Actual | Verdict | Log |
| --- | --- | --- | --- | --- | --- | --- |
| M1 | | | | | | |
| … | | | | | | |

## Reporting back

Do not change source. Create a branch `verification/macos-<YYYY-MM-DD>`, add `verification/macos-<date>.md` containing the environment table, the baseline results, and the evidence table with pasted logs, then open a pull request against the same base branch as this ticket. That pull request carries evidence only.

## Non-goals

No source changes, no fixes, and no design work. The behaviour these probes touch (reparse semantics, lock identity, path canonicalization) is owned by the platform effort; a `KNOWN-OPEN` row is a useful result.

**Status:** ready-for-agent
