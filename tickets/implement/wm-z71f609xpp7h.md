---
schema: 1
id: wm-z71f609xpp7h
title: MCP adapter over the shared library
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

Agents get the same capabilities as the command line, through MCP tools over the same shared library: the same validations, the same envelopes, the same error codes, and no reduced surface.

## Acceptance criteria

- [ ] the tool catalog exposes project, configuration, read, query, create, edit, lifecycle, relationship, archive, migration, diagnostics, and history capabilities
- [ ] business errors reuse the shared registry codes and typed details; no second taxonomy is introduced
- [ ] success and failure use the shared versioned envelope, and JSON-RPC protocol errors stay separate from business errors
- [ ] writes accept no batch array and no multiple targets
- [ ] a capability added to the command line appears in the tool catalog with the same semantics
