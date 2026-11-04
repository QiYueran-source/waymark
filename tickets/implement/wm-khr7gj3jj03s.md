---
schema: 1
id: wm-khr7gj3jj03s
title: Versioning and migration
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:20Z
updated_at: 2026-11-03T23:52:22Z
closed_at: 2026-11-03T23:52:20Z
parent: wm-srxj8ztv1zfz
dependencies:
  - wm-cp5vw04g7w95
  - wm-xdhqrmjy3mf8
---
## Parent

wm-srxj8ztv1zfz — Waymark functional specification.

## What to build

Format compatibility becomes explicit and previewable: one command-managed generation governs configuration and work items, older items are read-only until migrated, and version, schema, and storage migrations each dry-run, prevalidate, and apply as one all-or-nothing operation.

## Acceptance criteria

- [ ] the configuration carries the format generation and the configuration revision as `major.minor`; only a migration can change the generation
- [ ] a work item whose generation is older than the project's is read-only for mediated writes with `ITEM_NEEDS_MIGRATION` or `ACTION_REQUIRES_MIGRATION`, while reads and `show` still work
- [ ] version, schema, and storage migrations each expose a dry run that performs the full read-only check and writes nothing, with no persisted plan or plan/apply handshake
- [ ] storage prevalidation rejects absolute or escaping paths, overlapping or case-equivalent targets, an existing non-empty target, symbolic links, and a stale projection before any write
- [ ] schema removal deletes the key from every work item including archived ones; adding a required field applies its default so no item is left invalid
- [ ] execution failures report per-target outcomes, and a rollback that cannot complete is reported as `ROLLBACK_FAILED`
- [ ] a migration outside a Git repository proceeds with a warning that Git-based recovery is unavailable
