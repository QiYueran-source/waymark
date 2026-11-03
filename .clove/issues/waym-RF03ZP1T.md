---
schema: 1
id: waym-RF03ZP1T
title: 'Editing: title, body, and custom fields'
status: open
type: feature
priority: 2
created: 2026-11-03T05:43:38.246Z
updated: 2026-11-03T05:43:38.246Z
parent: waym-WMSPEC01
labels: [ready-for-agent]
deps: [waym-0XVE8PGE]
relates: []
duplicates: []
supersedes: []
---
## Parent

waym-WMSPEC01 — Waymark functional specification.

## What to build

Ordinary editing of a work item is split into three operations, each changing only its own data area: replacing the title, replacing or clearing the body, and setting or clearing custom fields in one validated change set. A request that changes nothing writes nothing.

## Acceptance criteria

- [ ] title replacement trims surrounding whitespace, requires a non-empty single line, and preserves internal whitespace
- [ ] body replacement clears on empty input, canonicalizes a non-empty body to exactly one trailing newline, and never trims leading or internal whitespace
- [ ] a custom-field update accepts mixed set and clear entries, rejects null, unknown keys, duplicate keys, set-and-clear conflicts, and clearing a required field, and validates the whole change set before one write
- [ ] an update with no effective change performs no write and leaves `updated_at` untouched
- [ ] an operator/owner mismatch emits an `OWNER_MISMATCH` warning and still writes
- [ ] edited documents remain byte-identical when re-serialized unchanged