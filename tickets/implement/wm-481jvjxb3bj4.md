---
schema: 1
id: wm-481jvjxb3bj4
title: Git history, diff, and generated agent documentation
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:21Z
updated_at: 2026-11-03T23:52:23Z
closed_at: 2026-11-03T23:52:21Z
parent: wm-srxj8ztv1zfz
dependencies:
  - wm-b9ac6qtng58a
---
## Parent

wm-srxj8ztv1zfz — Waymark functional specification.

## What to build

Change inspection leverages the repository instead of inventing a second history: history and diff over work items where Git is available, an informational result where it is not, and generated agent-facing documentation describing the interface.

## Acceptance criteria

- [ ] history and diff are available through both interfaces where a Git repository exists
- [ ] outside a Git repository the capability reports `NOT_A_GIT_REPOSITORY` as an informational finding rather than failing the whole command
- [ ] no separate audit log, attribution, or history store is introduced
- [ ] generated agent documentation describes the available capabilities without adding semantics of its own
