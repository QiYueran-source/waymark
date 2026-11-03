---
schema: 1
id: wm-5ax64rzs0m0d
title: 'Walking skeleton: initialize, create, and read a work item'
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:20Z
updated_at: 2026-11-03T23:52:21Z
closed_at: 2026-11-03T23:52:20Z
parent: wm-srxj8ztv1zfz
---
## Parent

wm-srxj8ztv1zfz — Waymark functional specification.

## What to build

A project can be initialized, one work item created, and that item read back by its stable ID, entirely through the CLI. Initialization creates the Waymark metadata directory and the project configuration document. Creation validates title, category, and owner, generates a stable ID, and writes one Markdown document with typed YAML frontmatter. Reading returns it by stable ID. Every write is a validated atomic replacement taken under the store-wide write lock, and serialization is deterministic.

## Acceptance criteria

- [ ] initialization creates the project configuration and its metadata directory
- [ ] creation requires title, category, and owner with no derivation, and a missing one fails the whole create
- [ ] the written document carries exactly the declared v1 frontmatter fields plus the body, in one fixed key order, with unset optional fields omitted rather than written as null
- [ ] re-serializing an unchanged work item produces byte-identical output
- [ ] reading returns one item by stable ID with its fields, body, and relationships, in human and versioned-JSON form
- [ ] a single-document write is validated and atomically replaced under the write lock
