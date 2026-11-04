---
schema: 1
id: wm-exzem3rwbmc6
title: Implement configuration and frontmatter migration
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:21Z
updated_at: 2026-11-03T23:52:22Z
closed_at: 2026-11-03T23:52:21Z
parent: wm-y03fa0a9bfbz
dependencies:
  - wm-17g2jqzhc935
  - wm-bn5d8k7zd729
---
## Parent

[Waymark general-purpose tracker functional specification](wm-y03fa0a9bfbz)

## What to build

As a project maintainer, I can evolve configuration and work-item formats safely, so older repository data can be upgraded without silent loss or accidental downgrade.

## Acceptance criteria

- [ ] One command-managed config_version governs configuration and frontmatter compatibility.
- [ ] Explicit migration supports dry-run, affected-file reporting, full prevalidation, backups, and deterministic output.
- [ ] Backup failure prevents migration writes; replacements are atomic per file and recovery failures are reported.
- [ ] Older data can be lazily upgraded on write; future-version and invalid data are rejected.
- [ ] Archive data is stored under the Waymark metadata area and migration diagnostics use the shared error contract.

## Blocked by

- T1: Establish Rust core and native work-item file loop.
- T2: Establish typed errors, error codes, envelopes, and diagnostics.
