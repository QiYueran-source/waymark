---
schema: 1
id: wm-w69renycvb2p
title: Define cross-platform paths and Git behavior
category: map
status: open
owner: frank
created_at: 2026-11-05T00:53:10Z
updated_at: 2026-11-05T02:17:34Z
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


- Use `/` separators in persisted storage/category paths and migration targets on every platform; reject absolute and drive-relative forms, backslashes without conversion, empty components and `..`. The standalone `.` may represent the project root, but which fields permit it remains open.
- Reject Windows-reserved device names case-insensitively including extensions, Windows-forbidden and control characters, and components ending in space or dot, on every platform. The complete rules and native validation remain to be established.
- Allow Unicode, such as `工作项/设计`, subject to all other rules; do not assume names that differ only in case or Unicode representation are distinct on all filesystems.
- Git work-item paths, storage-root paths and history path filters are literal paths, not wildcard or Git pathspec expressions. Directories still scope their contents; validation and native verification remain open.

## Still to resolve

- Exhaustive Windows reserved-name and character rules, permitted use of standalone `.`, case/Unicode equivalence, and handling existing incompatible configurations.
- Path traversal, reparse points, project-root aliases, and collision checks in the context of existing lock and migration decisions.
- Git path arguments, discovery, untracked single-item edge cases, error behavior and native verification.
