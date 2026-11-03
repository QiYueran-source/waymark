---
schema: 1
id: waym-Y1ZTWK25
title: Collection queries, search, and statistics
status: open
type: feature
priority: 2
created: 2026-11-03T05:43:41.984Z
updated: 2026-11-03T05:43:41.984Z
parent: waym-WMSPEC01
labels: [ready-for-agent]
deps: [waym-PM9HY6BD, waym-Q658VGH3]
relates: []
duplicates: []
supersedes: []
---
## Parent

waym-WMSPEC01 — Waymark functional specification.

## What to build

The read surface reaches parity with the specification: one filter, sort, pagination, and projection vocabulary across list, ready, and blocked; a structured query form; text search that prefers title hits; and counts-only statistics.

## Acceptance criteria

- [ ] list, ready, and blocked share one filter set, one sort vocabulary, and one pagination model
- [ ] default `rank` ordering is status group, then dependency topology, then stable ID, with a deterministic tie-break
- [ ] the structured query form is available where the interface is not already structured, and text search matches title and body case-insensitively with title hits first and no snippets
- [ ] statistics report counts only, always state archive range and projection completeness, and never report history or throughput
- [ ] pagination is snapshot-local: a page neither skips nor repeats items
- [ ] results carry source, completeness, freshness, and diagnostics metadata