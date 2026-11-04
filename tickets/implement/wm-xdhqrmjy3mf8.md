---
schema: 1
id: wm-xdhqrmjy3mf8
title: 'Configuration surface: read, validate, and declare the project'
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:20Z
updated_at: 2026-11-03T23:52:21Z
closed_at: 2026-11-03T23:52:20Z
parent: wm-srxj8ztv1zfz
dependencies:
  - wm-5212df47s7mx
---
## Parent

wm-srxj8ztv1zfz — Waymark functional specification.

## What to build

A project's configuration becomes fully manageable through commands: reading the effective configuration, validating it, setting scalars, adding, renaming, and removing categories, and declaring custom fields. Configuration writes never touch work-item files, the configuration revision counts effective writes, and invalid configuration is reported as locatable issues rather than a degraded run.

## Acceptance criteria

- [ ] the configuration commands cover reading, validating, scalar setting, category add/set/remove, and custom-field add/set
- [ ] an unused category can be removed; a category still used by work items is refused after a read-only reference check that changes no work-item file
- [ ] configuration validation reports per-issue `CONFIG_INVALID` findings for key grammar, path escape or overlap, symbolic links, an empty display name, an unknown field type, and a default that does not match its declared type
- [ ] a mediated configuration write that changes content increments the revision by exactly one; a no-op write does not
- [ ] adding an optional custom field rewrites no work-item file and leaves the field unset on existing items
