---
schema: 1
id: waym-0XVE8PGE
title: 'Configuration surface: read, validate, and declare the project'
status: closed
type: feature
priority: 2
created: 2026-11-03T05:43:38.227Z
updated: 2026-11-03T16:50:39.429Z
closed: 2026-11-03T16:50:39.429Z
parent: waym-WMSPEC01
labels: [ready-for-agent]
deps: [waym-0GQKACVA]
relates: []
duplicates: []
supersedes: []
---
## Parent

waym-WMSPEC01 — Waymark functional specification.

## What to build

A project's configuration becomes fully manageable through commands: reading the effective configuration, validating it, setting scalars, adding, renaming, and removing categories, and declaring custom fields. Configuration writes never touch work-item files, the configuration revision counts effective writes, and invalid configuration is reported as locatable issues rather than a degraded run.

## Acceptance criteria

- [ ] the configuration commands cover reading, validating, scalar setting, category add/set/remove, and custom-field add/set
- [ ] an unused category can be removed; a category still used by work items is refused after a read-only reference check that changes no work-item file
- [ ] configuration validation reports per-issue `CONFIG_INVALID` findings for key grammar, path escape or overlap, symbolic links, an empty display name, an unknown field type, and a default that does not match its declared type
- [ ] a mediated configuration write that changes content increments the revision by exactly one; a no-op write does not
- [ ] adding an optional custom field rewrites no work-item file and leaves the field unset on existing items