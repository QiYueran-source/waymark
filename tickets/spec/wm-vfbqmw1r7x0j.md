---
schema: 1
id: wm-vfbqmw1r7x0j
title: Waymark Windows platform support
category: spec
status: closed
owner: frank
created_at: 2026-11-04T15:31:05Z
updated_at: 2026-11-05T06:48:04Z
closed_at: 2026-11-05T06:48:04Z
---
## Status

Superseded on 2026-11-05. This specification has been absorbed into [Waymark functional specification](wm-srxj8ztv1zfz.md), which is now the single normative document and states platform behaviour as explicit per-platform requirements under "Platform support". The replaced text of this item remains in Git history only; nothing here is normative any more.

## What replaced it

- **Scope.** Windows was MSVC-only with the GNU toolchain out of scope. Both `x86_64-pc-windows-msvc` and `x86_64-pc-windows-gnu` are supported targets, built and tested natively; only the MSVC artifact is published.
- **Compilation.** The unconditional Unix error constants were to fall back to a generic IO error and a `symlink_metadata` precheck. Error classification is now capability-based and names no POSIX constant outside `cfg(unix)`, and the lock decision must be made from the opened handle where the platform allows it, because a check before opening is not sufficient.
- **Dependencies.** No Windows-specific dependency was to be added. A maintained official binding limited to `cfg(windows)` is now permitted, because owner and DACL preservation, reparse-tag inspection, handle-based volume identity and no-replace moves are not expressible with the standard library alone.
- **Testing.** The existing `#[cfg(unix)]` tests were to stay gated. A skip must now be registered in a version-controlled allowlist with a stable identifier and reason, an unlisted skip fails the leg, and native Windows coverage is required for lock contention, portable path validation, reparse refusal, replacement, rollback and migration.
- **Behaviour.** Beyond parity for the CLI, MCP and Web, the functional specification now also covers untracked work-item diffs on Windows, portable path rules, the fixed-version Unicode comparison, migration volume and no-overwrite safety, three-state rollback reporting, directory-synchronization warnings, and bounded temporary-and-recovery diagnostics.

## Where the rest lives

The packaged archive contents, the release matrix, `serve --open`, the loopback-only Web binding and the requirement that Windows runs in pull-request CI now live in the functional specification. The release pipeline itself remains owned by [Waymark distribution, packaging, and release](wm-4125jxq7bpq8), which now carries the two Windows CI legs and the MSVC artifact.

## Evidence

No Windows build, test run or packaged artifact exists yet. This item's replacement states requirements, not verified support.
