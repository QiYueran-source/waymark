---
schema: 1
id: waym-3Y19NH3K
title: 'Walking skeleton: initialize, create, and read a work item'
status: open
type: feature
priority: 2
created: 2026-11-03T05:43:10.781Z
updated: 2026-11-03T05:43:10.781Z
parent: waym-WMSPEC01
labels: [ready-for-agent]
deps: []
relates: []
duplicates: []
supersedes: []
---
## Parent

waym-WMSPEC01 — Waymark functional specification.

## What to build

A project can be initialized, one work item created, and that item read back by its stable ID, entirely through the CLI. Initialization creates the Waymark metadata directory and the project configuration document. Creation validates title, category, and owner, generates a stable ID, and writes one Markdown document with typed YAML frontmatter. Reading returns it by stable ID. Every write is a validated atomic replacement taken under the store-wide write lock, and serialization is deterministic.

## Acceptance criteria

- [ ] initialization creates the project configuration and its metadata directory
- [ ] creation requires title, category, and owner with no derivation, and a missing one fails the whole create
- [ ] the written document carries exactly the declared v1 frontmatter fields plus the body, in one fixed key order, with unset optional fields omitted rather than written as null
- [ ] re-serializing an unchanged work item produces byte-identical output
- [ ] reading returns one item by stable ID with its fields, body, and relationships, in human and versioned-JSON form
- [ ] a single-document write is validated and atomically replaced under the write lock