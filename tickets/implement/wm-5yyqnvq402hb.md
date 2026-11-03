---
schema: 1
id: wm-5yyqnvq402hb
title: 'Editing: title, body, and custom fields'
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:21Z
updated_at: 2026-11-03T23:52:22Z
closed_at: 2026-11-03T23:52:21Z
parent: wm-srxj8ztv1zfz
dependencies:
  - wm-xdhqrmjy3mf8
---
## Parent

wm-srxj8ztv1zfz — Waymark functional specification.

## What to build

Ordinary editing of a work item is split into three operations, each changing only its own data area: replacing the title, replacing or clearing the body, and setting or clearing custom fields in one validated change set. A request that changes nothing writes nothing.

## Acceptance criteria

- [ ] title replacement trims surrounding whitespace, requires a non-empty single line, and preserves internal whitespace
- [ ] body replacement clears on empty input, canonicalizes a non-empty body to exactly one trailing newline, and never trims leading or internal whitespace
- [ ] a custom-field update accepts mixed set and clear entries, rejects null, unknown keys, duplicate keys, set-and-clear conflicts, and clearing a required field, and validates the whole change set before one write
- [ ] an update with no effective change performs no write and leaves `updated_at` untouched
- [ ] an operator/owner mismatch emits an `OWNER_MISMATCH` warning and still writes
- [ ] edited documents remain byte-identical when re-serialized unchanged
