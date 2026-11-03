---
schema: 1
id: waym-N0NGA6QM
title: Implement configuration and frontmatter migration
status: closed
type: feature
priority: 2
created: 2026-11-02T12:09:28.206Z
updated: 2026-11-03T03:15:31.235Z
closed: 2026-11-03T03:15:31.235Z
parent: waym-DZ3Q9D11
labels: [ready-for-agent]
deps: [waym-3HSJH4RT, waym-419ZAWN7]
relates: []
duplicates: []
supersedes: []
---
## Parent

[Waymark general-purpose tracker functional specification](waym-DZ3Q9D11)

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