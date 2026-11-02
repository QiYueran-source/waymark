---
schema: 1
id: waym-3HSJH4RT
title: Establish Rust core and native work-item file loop
status: open
type: feature
priority: 2
created: 2026-11-02T12:09:28.145Z
updated: 2026-11-02T12:09:28.145Z
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
- [ ] Work items have stable IDs, category, creator, human owner, lifecycle fields, timestamps, and body.
- [ ] Create, show, and edit operations preserve deterministic serialization and Git-readable files.
- [ ] Invalid core input returns typed structured errors rather than process panics.

## Blocked by

None (can start immediately).