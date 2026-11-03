---
schema: 1
id: wm-b9ac6qtng58a
title: Collection queries, search, and statistics
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:21Z
updated_at: 2026-11-03T23:52:23Z
closed_at: 2026-11-03T23:52:21Z
parent: wm-srxj8ztv1zfz
dependencies:
  - wm-8b80a5bk80wf
  - wm-q90azj65xneg
---
## Parent

wm-srxj8ztv1zfz — Waymark functional specification.

## What to build

The read surface reaches parity with the specification: one filter, sort, pagination, and projection vocabulary across list, ready, and blocked; a structured query form; text search that prefers title hits; and counts-only statistics.

## Acceptance criteria

- [ ] list, ready, and blocked share one filter set, one sort vocabulary, and one pagination model
- [ ] default `rank` ordering is status group, then dependency topology, then stable ID, with a deterministic tie-break
- [ ] the structured query form is available where the interface is not already structured, and text search matches title and body case-insensitively with title hits first and no snippets
- [ ] statistics report counts only, always state archive range and projection completeness, and never report history or throughput
- [ ] pagination is snapshot-local: a page neither skips nor repeats items
- [ ] results carry source, completeness, freshness, and diagnostics metadata
