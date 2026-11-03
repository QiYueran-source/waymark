---
schema: 1
id: waym-ZH9RGQRA
title: Git history, diff, and generated agent documentation
status: open
type: feature
priority: 2
created: 2026-11-03T05:43:42.101Z
updated: 2026-11-03T05:43:42.101Z
parent: waym-WMSPEC01
labels: [ready-for-agent]
deps: [waym-Y1ZTWK25]
relates: []
duplicates: []
supersedes: []
---
## Parent

waym-WMSPEC01 — Waymark functional specification.

## What to build

Change inspection leverages the repository instead of inventing a second history: history and diff over work items where Git is available, an informational result where it is not, and generated agent-facing documentation describing the interface.

## Acceptance criteria

- [ ] history and diff are available through both interfaces where a Git repository exists
- [ ] outside a Git repository the capability reports `NOT_A_GIT_REPOSITORY` as an informational finding rather than failing the whole command
- [ ] no separate audit log, attribution, or history store is introduced
- [ ] generated agent documentation describes the available capabilities without adding semantics of its own