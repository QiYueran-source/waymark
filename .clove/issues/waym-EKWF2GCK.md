---
schema: 1
id: waym-EKWF2GCK
title: Versioning and migration
status: closed
type: feature
priority: 2
created: 2026-11-03T05:43:42.021Z
updated: 2026-11-03T16:50:39.623Z
closed: 2026-11-03T16:50:39.623Z
parent: waym-WMSPEC01
labels: [ready-for-agent]
deps: [waym-0XVE8PGE, waym-940D5VC9]
relates: []
duplicates: []
supersedes: []
---
## Parent

waym-WMSPEC01 — Waymark functional specification.

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