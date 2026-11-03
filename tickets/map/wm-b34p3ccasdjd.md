---
schema: 1
id: wm-b34p3ccasdjd
title: Define the query and reporting surface
category: map
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:21Z
updated_at: 2026-11-03T23:52:22Z
closed_at: 2026-11-03T23:52:21Z
parent: wm-y14hs95k8wct
---
## Question

What external shape do the read and reporting operations expose?

Underlying semantics are settled: deterministic ordering with stable-ID tie breakers, snapshot-local pagination, active-only collections by default, explicit archived-only selection, and a finite custom-field filter vocabulary. What is not settled:

- which operations exist as distinct surfaces (`list`, `tree`, `children`, `leaves`, `ready`, `blocked`, `search`, `query`, `stats`) and which are redundant;
- the parameter naming and shape for filters, ordering, field projection, pagination, and archive range;
- whether tree queries take a root, a depth, or a category grouping switch;
- what `search` matches, how results rank, and whether it is case-sensitive;
- what `stats` reports: which dimensions, which counting rules, and whether completion or readiness is part of it;
- how these surfaces behave when the projection is incomplete or a generation is stale.
