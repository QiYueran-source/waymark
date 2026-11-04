---
schema: 1
id: wm-577fs5mjdfe3
title: Incomplete-projection message names the actual shortfall
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-04T01:02:47Z
updated_at: 2026-11-04T11:30:23Z
closed_at: 2026-11-04T11:30:23Z
---
## What to build

`INCOMPLETE_PROJECTION` currently reads "a complete, fresh projection is required but the projection is {completeness} and {freshness}". With a fresh but scoped projection that renders as "the projection is scoped and fresh", which reads as a contradiction: both halves describe the same object, and the requirement ("complete, fresh") is restated as if freshness were the problem. Readers cannot tell what is actually missing.

Rewrite the message so it states the shortfall and its concrete cause: what the reader asked for, what the projection is, and how many documents or inventory paths blocked it. Keep it one line of prose suitable for a CLI error and a Web failure block.

## Acceptance criteria

- [ ] a scoped-and-fresh projection produces a message that says the projection is incomplete and names how many documents could not be read and how many inventory paths are uncertain
- [ ] a stale-but-complete projection produces a message that names staleness, not incompleteness
- [ ] a projection that is both scoped and stale names both shortfalls without repeating "complete, fresh"
- [ ] the error code, exit class, retryability, and the `completeness`/`freshness`/`diagnostics` details keys are unchanged
- [ ] the registry golden snapshot and constructor-coverage tests still pass unchanged
- [ ] tests assert the message distinguishes incomplete from stale, rather than pinning one exact sentence
