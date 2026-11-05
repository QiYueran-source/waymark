---
schema: 1
id: wm-jmckd658s129
title: Windows native verification plan
category: spec
status: open
owner: frank
creator: frank
created_at: 2026-11-05T15:09:22Z
updated_at: 2026-11-05T15:09:22Z
parent: wm-vfbqmw1r7x0j
---
## Parent

[Waymark Windows platform support](wm-vfbqmw1r7x0j)

## What this is

A native verification plan for a Windows machine. It is a runbook for an agent or a person who clones this repository on Windows and reports what the product actually does there; it is **not** an implementation ticket.

CI already runs the full gate on `windows-latest`. A native machine adds what a hosted runner cannot cover reliably: reparse points and junctions, ACLs and share modes, long paths, drive letters, case-insensitive filesystems, Git for Windows path handling, a real browser for `waymark serve`, and the packaged `.exe` names.

## Rules for the operator

- Do not modify source code, tests, or configuration in this repository. The deliverable is evidence, not a fix.
- Every probe is either **PASS**, **FAIL**, **ENV-LIMITED** (the machine cannot perform the operation, e.g. no Developer Mode), or **KNOWN-OPEN** (the behaviour is a known gap owned by the platform effort). Say which, every time.
- Capture the exact command, its exit code, and enough output to reproduce.
- If a probe fails, keep going. One failing probe does not stop the run.

## Environment to record first

| Fact | Command |
| --- | --- |
| Windows version and build | `ver` and `[System.Environment]::OSVersion.Version` in PowerShell |
| Architecture | `uname -m` in Git Bash, or `$env:PROCESSOR_ARCHITECTURE` |
| Developer Mode (needed for symlinks) | Settings → Privacy & security → For developers; record on/off |
| Rust | `rustc --version`, `cargo --version`, `rustup toolchain list` |
| Node | `node --version`, `npm --version` |
| Git | `git --version`, `git config --get core.autocrlf`, `git config --get core.ignorecase` |
| Filesystem of the checkout volume | `fsutil fsinfo volumeinfo C:` (or the drive actually used) |
| Shell | PowerShell 7 vs Windows PowerShell 5.1 vs Git Bash; record all that are used |

Prerequisites: `git`, Rust stable **and** 1.85.0, Node 22, and a writable build location. On a managed host the default `%USERPROFILE%\.cargo` may be read-only, so set:

```powershell
$env:CARGO_HOME = "D:\waymark-build\cargo-home"
$env:CARGO_TARGET_DIR = "D:\waymark-build\target"
```

Network is needed for the first `cargo fetch` / `npm ci`. If it is unavailable, record that and stop; a probe that cannot run is `ENV-LIMITED`, not `FAIL`.

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

```powershell
cd waymark\crates\waymark-web\ui
npm ci
npm run check
npm run build
npm test
cd ..\..\..\..\..
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

### W1 — atomic replacement over an existing file

```bash
cd "$(mktemp -d)"
waymark init
waymark create --title "one" --category task --owner frank
waymark edit <id> --title "two"
waymark show <id> --json
Get-ChildItem -Recurse -Filter *.waymark-tmp    # expect nothing
```

Expected: the title changes, no staging file is left behind.

### W2 — concurrent writers do not lose work

```powershell
1..8 | ForEach-Object -Parallel {
  waymark create --title "parallel $_" --category task --owner frank
} -ThrottleLimit 8
waymark list --json
```

Expected: eight documents, `list` reports eight, no `*.waymark-tmp`, and any failure is a typed `LOCK_TIMEOUT`/`WRITE_CONFLICT` rather than a torn document.

### W3 — a symlinked local document is not written through

```powershell
New-Item -ItemType SymbolicLink -Path .waymark\user.toml -Target "$env:TEMP\elsewhere.toml" -Force
waymark config set operator frank
Get-Content "$env:TEMP\elsewhere.toml"
```

Expected: refused (`LOCK_FAILED` or `CONFIG_INVALID`) and the target unchanged. If creating the link fails with a privilege error, record `ENV-LIMITED` and say whether Developer Mode was off.

### W4 — a junctioned category directory

```powershell
New-Item -ItemType Junction -Path work\junctioned -Target "$env:TEMP\real-category"
# declare it: add [categories.junctioned] path = "work/junctioned" to .waymark/config.toml by hand
waymark --json validate
```

Expected from the specification: a symbolic-link/reparse refusal finding. **Known open item:** a Windows junction may not be detected as a symlink, so record the actual result as `KNOWN-OPEN` rather than `FAIL` when no finding appears, and paste the finding or the silence verbatim.

### W5 — case-insensitive collisions

```powershell
$id = waymark create --title "case" --category task --owner frank   # capture the id from JSON
Copy-Item "work\task\$id.md" "work\task\$($id.ToUpper()).md"
waymark --json validate
```

Expected: a duplicate-ID or case-variant finding. Record exactly which, and whether the second file survived on disk.

### W6 — long path

Create a category whose path is 240+ characters (nested directories), then `waymark create` in it and `waymark --json validate`.

Expected: success, or a clear `IO_ERROR` naming the path. Record the total length at which it changes behaviour.

### W7 — a hand edit with CRLF

```powershell
(Get-Content "work\task\$id.md" -Raw) -replace "`n", "`r`n" | Set-Content "work\task\$id.md" -NoNewline
waymark show $id --json
waymark --json validate
```

Expected: the document still parses and its field values are unchanged. Record whether the next Waymark write normalizes the file back to LF.

### W8 — Git for Windows paths

```bash
git init; git add -A; git commit -m init
waymark create --title "untracked" --category task --owner frank
waymark git diff <new-id> --json     # expect a nonempty new-file patch
waymark git history <id> --json      # expect "/" separators in paths
```

Expected: a nonempty patch for the untracked document and `/` in every reported path.

### W9 — the Web view in a real browser

```bash
waymark serve --port 0 --open
```

Expected: the browser opens, the tree renders, and the detail panel's **Created** line shows the machine's zone name in parentheses plus an offset. Compare it with `waymark config show` (effective zone). Quit with Ctrl-C.

### W10 — packaged binary names (only if a release artifact is supplied)

```powershell
.\waymark.exe version
.\waymark-mcp.exe --help
```

Expected: the version line reports its capabilities; the MCP binary starts and exits cleanly on `--help`.

## Evidence table (fill this in)

| Probe | Command | Exit | Expected | Actual | Verdict | Log |
| --- | --- | --- | --- | --- | --- | --- |
| W1 | | | | | | |
| … | | | | | | |

## Reporting back

Do not change source. Create a branch `verification/windows-<YYYY-MM-DD>`, add `verification/windows-<date>.md` containing the environment table, the baseline results, and the evidence table with pasted logs, then open a pull request against the same base branch as this ticket. That pull request carries evidence only.

## Non-goals

No source changes, no fixes, and no design of Windows-specific semantics. The reparse-point, ACL, and lock behaviour this plan probes is owned by the platform effort; a `KNOWN-OPEN` row is a useful result, not a failure of the run.

**Status:** ready-for-agent
