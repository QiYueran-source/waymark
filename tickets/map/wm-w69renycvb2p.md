---
schema: 1
id: wm-w69renycvb2p
title: Define cross-platform paths and Git behavior
category: map
status: open
owner: frank
created_at: 2026-11-05T00:53:10Z
updated_at: 2026-11-05T02:15:49Z
parent: wm-4wbvzsv34g2p
dependencies:
  - wm-78d0sa7jaxpb
---
## Type

grilling

## Question

How should project-relative paths and work-item files remain safe and portable across supported systems, including Windows drive prefixes, reserved names and case behavior; what parity is required for Git operations, including untracked diffs?

## Confirmed decisions

- Persisted storage-root and category paths and storage-migration targets use a common cross-platform lexical validation policy, requiring a safe project-relative representation. It does not constrain the project's actual absolute path. Exact syntax and existing incompatible configurations remain unresolved.
- A working-tree diff for a specified existing, untracked work-item file produces a nonempty whole-new-file patch across Linux, macOS, Windows MSVC and GNU when Git is available, rather than returning empty based on platform. Special cases and implementation remain unresolved.
- A storage-root working-tree diff without an item continues to include tracked files only. Explain that an empty patch does not prove that untracked files are absent; a user can request a single untracked item by ID.

## Still to resolve

- Exact portable path syntax, Windows reserved names and case/Unicode equivalence; handling existing incompatible configurations.
- Path traversal, reparse points, project-root aliases, and collision checks in the context of existing lock and migration decisions.
- Git path arguments, discovery, untracked single-item edge cases, error behavior and native verification.
