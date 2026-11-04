---
schema: 1
id: wm-vfbqmw1r7x0j
title: Waymark Windows platform support
category: spec
status: open
owner: frank
created_at: 2026-11-04T15:31:05Z
updated_at: 2026-11-04T15:31:47Z
---
# Waymark Windows platform support

## Problem Statement

Waymark builds and runs on Linux and macOS but not on Windows. `waymark-core` declares `libc` only for unix targets, while two call sites use Unix-only constants unconditionally: `libc::ELOOP` when opening the store-wide write lock, and `libc::EXDEV` when a storage migration's rename crosses filesystems. A `windows-msvc` build therefore fails to compile before the packaging question is even reached.

Fixing compilation alone is not enough to call Windows supported. The write lock's behaviour comes from `fd-lock` (`LockFileEx` on Windows), and the atomic replace path is a temp-file rename — neither is exercised by any test on Windows today. A binary that compiles but whose lock and rename semantics are unproven is worse than no binary: it can corrupt a user's repository. No CI job runs the suite on Windows, and the release pipeline produces no Windows artifact.

## Solution

Make Windows a first-class, tested platform with exactly one target and no change to Unix behaviour.

The two Unix-specific error classifications get portable fallbacks: Unix keeps `O_NOFOLLOW`, the `ELOOP` message, and the `EXDEV` message; non-Unix refuses a symlinked lock path by inspecting `symlink_metadata` before the open, and lets a failed rename fall through to the generic IO error rather than adding a `windows-sys` dependency for a message string. The existing `#[cfg(unix)]` tests stay gated; the suite as a whole must pass on `windows-latest` in PR CI, not only at release time.

The release pipeline gains a `windows-latest` runner producing `waymark-v<version>-windows-x86_64.zip` and `waymark-web-v<version>-windows-x86_64.zip`, containing `waymark.exe` and `waymark-mcp.exe` plus the same documentation, licences, and checksum sibling as the Unix archives. The shared pipeline, artifact naming, provenance checks, and gates are owned by [Waymark distribution, packaging, and release](wm-4125jxq7bpq8); this specification adds Windows to them.

## User Stories

1. As a Windows user, I download `waymark-v<version>-windows-x86_64.zip`, verify its `.sha256`, extract it, and run `waymark.exe version`, `waymark.exe init`, `waymark.exe new`, and `waymark.exe list` in a project without installing Rust.
2. As a Windows user, I download the Web package, run `waymark.exe serve`, and get the read-only Web view in a browser at a loopback URL.
3. As a Windows user, I see the same canonical work-item documents, the same JSON envelopes, and the same exit codes as on Linux and macOS.
4. As a Windows user, I can rely on concurrent writes being serialised by the same store-wide lock, with a held lock producing `LOCK_TIMEOUT` rather than a corrupted file.
5. As a Windows user, a symlinked `.waymark/write.lock` is refused rather than followed, and the error names the problem.
6. As a Windows user, a storage migration whose rename crosses volumes fails with a clear IO error instead of half-moving the store.
7. As a Windows user following the MCP guide, I can point my client at `waymark-mcp.exe` over stdio and get the same tools as on Unix.
8. As a maintainer, a Windows compile break or test failure fails the pull request on `windows-latest`, not the release.
9. As a maintainer, the tagged release produces both Windows archives with checksums and includes them in the draft Release alongside the Unix ones.
10. As a maintainer, the release provenance check runs against the Windows Web archive too — start `waymark.exe serve` on a free port, fetch `/`, and compare its hash with the embedded manifest.
11. As a maintainer, I never have to special-case Windows in the release checklist beyond running the packaged smoke checks on a Windows machine.

## Implementation Decisions

- **One target:** `x86_64-pc-windows-msvc` only. No `aarch64-pc-windows-msvc`, no `x86_64-pc-windows-gnu`. The CI runner is `windows-latest`; the toolchain is the stable MSVC toolchain that runner provides.
- **Lock opening:** keep the `#[cfg(unix)]` branch exactly as it is (`O_NOFOLLOW`, `truncate(false)`, `mode(0o644)`, `ELOOP` message). Add a `#[cfg(not(unix))]` branch that calls `symlink_metadata` on the lock path first and returns `LOCK_FAILED` naming the symlink when one is found, then opens normally. A symlink created between the check and the open falls through to the ordinary IO error; that race is accepted rather than closed with a Windows-specific open flag, because the lock file is created by Waymark inside the project, not authored by the user.
- **Rename classification:** keep the `libc::EXDEV` message under `#[cfg(unix)]`; on non-Unix the rename failure is reported by the generic IO error. No `windows-sys`/`winapi` dependency is added for one error code.
- **`serve --open` on Windows:** the distribution specification defines `serve --open`; this spec fixes its Windows implementation to `cmd /C start "" <url>`, with the URL passed as a separate argument so shell metacharacters in it are not reinterpreted. Failure to launch the browser is a warning, never a serve failure.
- **CI:** a `windows-latest` job runs `cargo fmt --all --check`, `cargo test --workspace --all-features`, and `cargo clippy --workspace --all-targets --all-features -- -D warnings` on every pull request and on the release tag. The job is a peer of the Linux and macOS jobs, not a release-only extra.
- **Release matrix:** the Windows entry sets `os: windows-latest`, `target: x86_64-pc-windows-msvc`, builds both variants, and packs `.zip` archives with `waymark.exe` and `waymark-mcp.exe`; the packaging step is shared with the Unix entries apart from the archive command and the `.exe` suffixes.
- **Tests that cannot run on Windows:** the existing `#[cfg(unix)]` symlink and permission tests stay gated. Windows receives its own coverage of the lock's contention and `LOCK_TIMEOUT` behaviour through the existing multi-process test, which is platform-neutral; a filesystem-level symlink test is attempted only when the runner can create symlinks (developer mode or an elevated runner), and its absence is recorded rather than silently skipped.
- **No behaviour fork:** no Windows-specific config fields, path rules, or error codes. `Utf8Path` stays the path type everywhere; a non-UTF-8 project path is rejected identically on every platform.

## Testing Decisions

- Run the full workspace suite on `windows-latest` in PR CI and on the release tag; a failure blocks both.
- Add a Windows-run test that takes the write lock from a second process and asserts `LOCK_TIMEOUT`, proving `fd-lock`'s `LockFileEx` path behaves like `flock` under contention.
- Add a Windows-run test that the lock refuses a symlink where the runner permits creating one; skip with an explicit recorded reason otherwise.
- Smoke the packaged Windows archives from the draft Release: `waymark.exe version`, `init`, `new`, `list`, `show --json`, and `serve --port 0` followed by a `GET /` hash comparison against the embedded manifest.
- Confirm the Windows archive holds `waymark.exe` and `waymark-mcp.exe`, the README, both licences, `THIRD_PARTY_LICENSES.txt`, and a matching `.sha256`, using the same packaging assertions as the Unix archives.
- Add a release-gate assertion that the Windows job was green for the tagged commit before the draft is published.

## Out of Scope

- Windows ARM64, the GNU toolchain, MSYS/Cygwin environments, and WSL-specific behaviour.
- MSI, MSIX, or any installer; a Windows service; auto-update.
- Code signing of the Windows binaries.
- Any change to Linux or macOS behaviour, error codes, or file layout.
- Windows-specific Web UI, MCP, or domain behaviour.
- Non-loopback binding on Windows.

## Further Notes

- The distribution specification owns the pipeline this spec plugs into; the Windows jobs must reuse its packaging, provenance, and checksum steps rather than forking them.
- Clove's release workflow includes a Windows entry with `7z` zipping and `.exe` copies; that shape is the reference here, but Waymark deliberately adds the PR-time Windows job that Clove keeps release-only.
- The design map [Waymark build, packaging, and distribution](wm-g2bzgcmzweaz) records the decisions this specification implements.
- This specification is ready for implementation planning once the distribution specification settles the shared pipeline.
