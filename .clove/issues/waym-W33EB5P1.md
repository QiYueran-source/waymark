---
schema: 1
id: waym-W33EB5P1
title: MCP adapter over the shared library
status: open
type: feature
priority: 2
created: 2026-11-03T05:43:42.063Z
updated: 2026-11-03T05:43:42.063Z
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

Agents get the same capabilities as the command line, through MCP tools over the same shared library: the same validations, the same envelopes, the same error codes, and no reduced surface.

## Acceptance criteria

- [ ] the tool catalog exposes project, configuration, read, query, create, edit, lifecycle, relationship, archive, migration, diagnostics, and history capabilities
- [ ] business errors reuse the shared registry codes and typed details; no second taxonomy is introduced
- [ ] success and failure use the shared versioned envelope, and JSON-RPC protocol errors stay separate from business errors
- [ ] writes accept no batch array and no multiple targets
- [ ] a capability added to the command line appears in the tool catalog with the same semantics