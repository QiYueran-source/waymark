---
schema: 1
id: waym-3HSJH4RT
title: Establish Rust core and native work-item file loop
status: closed
type: feature
priority: 2
created: 2026-11-02T12:09:28.145Z
updated: 2026-11-03T03:15:30.972Z
closed: 2026-11-03T03:15:30.972Z
parent: waym-DZ3Q9D11
labels: [ready-for-agent]
deps: []
relates: []
duplicates: []
supersedes: []
---
## Parent

[Waymark general-purpose tracker functional specification](waym-DZ3Q9D11)

## What to build

As a project maintainer, I can initialize a Waymark project and create, read, and edit a work item stored as authoritative Markdown with YAML frontmatter, so the repository has a usable native file loop.

## Acceptance criteria

- [ ] A Rust workspace and core domain module compile with automated tests.
- [ ] Project configuration defines a configurable storage root and category paths.
- [ ] Work items persist the v1 core fields `id`, `title`, `category`, `status`, `owner`, `creator`, `created_at`, `updated_at`, optional `closed_at`, `body`, optional `parent`, unique sorted `dependencies`/`soft_links`, and schema-validated `custom_fields`.
- [ ] Stable IDs use `wm-` plus 12 Crockford Base32 characters, persisted lowercase and parsed case-insensitively; generation detects collisions and ordinary create does not accept caller-supplied IDs.
- [ ] Categories use stable lowercase ASCII keys distinct from display names; work items persist keys and category changes/removals preserve referential integrity.
- [ ] Title is non-empty after trimming; owner/creator are non-empty declared identities with exact normalized comparison; timestamps are UTC RFC 3339 and system-maintained.
- [ ] Create, show, and edit operations preserve deterministic serialization and Git-readable files; unset optional values are omitted, not serialized as null.
- [ ] Unified edit supports optional full body replacement: absent body leaves it unchanged, empty body clears it, and non-empty body is canonicalized to exactly one trailing newline. CLI supports `$EDITOR`, `--body`, and `--body-file`; MCP uses structured body input without launching an editor.
- [ ] Body and frontmatter changes are validated and written as one fingerprint-checked atomic document replacement; ordinary body changes update `updated_at`, while archive does not.
- [ ] Invalid core input returns typed structured errors rather than process panics.

## Blocked by

None (can start immediately).