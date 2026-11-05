---
schema: 1
id: wm-78d0sa7jaxpb
title: Research Windows filesystem safety primitives
category: map
status: closed
owner: frank
created_at: 2026-11-05T00:53:09Z
updated_at: 2026-11-05T00:58:04Z
closed_at: 2026-11-05T00:58:04Z
parent: wm-4wbvzsv34g2p
---
## Type

research

## Question

Which documented Windows and Rust primitives and dependency behaviors can provide safe lock-file opening without following links or reparse points, process locks, atomic replacement, permissions, and migration preflight? Identify guarantees, gaps, Windows MSVC/GNU differences, and reliable tests using primary sources; distinguish existing behavior from proposals.

## Research context

Completed in local branch `research/waymark-platform-fs` at commit `d25023dd21be11cf5e5d832a43ea1e93acfb1efb`: [Windows filesystem safety primitives](../../.worktrees/research-platform-fs/research/windows-filesystem.md). The branch is not pushed or merged.

## Resolution

The existing check-then-open approach has a race; opening the final reparse point and checking the handle is a candidate, not proof about ancestor paths or hard links. `fd-lock` coordinates holders of the same file object; lock-path replacement and sharing need separate treatment. `tempfile` promises single-file atomic replacement but not batch atomicity or crash durability, and Windows replacement options have different failure and metadata behavior. Windows volume preflight and ACL handling require explicit contracts. The linked evidence lists sources, tests and open trade-offs. No Windows runtime tests were performed; no implementation choice was made.
