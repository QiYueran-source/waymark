---
schema: 1
id: wm-78d0sa7jaxpb
title: Research Windows filesystem safety primitives
category: map
status: open
owner: frank
created_at: 2026-11-05T00:53:09Z
updated_at: 2026-11-05T00:54:04Z
parent: wm-4wbvzsv34g2p
---
## Type

research

## Question

Which documented Windows and Rust primitives and dependency behaviors can provide safe lock-file opening without following links or reparse points, process locks, atomic replacement, permissions, and migration preflight? Identify guarantees, gaps, Windows MSVC/GNU differences, and reliable tests using primary sources; distinguish existing behavior from proposals.

## Research context

Pending: local branch `research/waymark-platform-fs`, planned findings at `research/windows-filesystem.md` on that branch. This branch is not pushed or merged.
