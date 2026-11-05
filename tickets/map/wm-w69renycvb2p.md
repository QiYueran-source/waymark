---
schema: 1
id: wm-w69renycvb2p
title: Define cross-platform paths and Git behavior
category: map
status: open
owner: frank
created_at: 2026-11-05T00:53:10Z
updated_at: 2026-11-05T02:39:39Z
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


- For configured category paths and storage-migration targets, reject pre-existing Windows reparse points of any kind, including junctions and mount points, extending the existing no-symlink policy. Actual migration refuses when target component safety cannot be established; root aliases remain permitted and adversarial ancestor replacement is not covered. Read behavior and other paths remain open.
- An explicitly requested existing ignored work item gets a whole-new-file patch; storage-root scope does not automatically include ignored or untracked files.
- Without `HEAD`, the working-tree diff baseline is the empty tree: an untracked item still has a whole-new-file patch, while staged item and storage-root files are compared to the empty baseline using their current working-tree state. Storage-root scope still omits untracked files. With `HEAD`, compare against it as before.


- Existing configurations incompatible with the new portable path rules are not silently renamed or allowed to continue writing as if valid. Report the config key, path and reason; retain configuration-only read diagnostics without suggesting the entire project was checked. Users back up, stop writes, coordinate directory and config changes and revalidate; editing only the configuration can hide work items. Existing migration cannot be assumed to repair invalid configuration. Repair guidance and any dedicated tool remain open.
- All platforms reject ASCII case-equivalent category paths in configuration and migration prechecks, including equality and ancestor/descendant overlap; identify conflicts and never select or overwrite one. Unicode and other filesystem aliases require additional policy.
- Git history/diff locate the actual containing working tree for projects in subdirectories, accessible through permitted project-root aliases, or in linked worktrees, and remain scoped to the requested project. Preserve informational empty results for unavailable Git or absence of a repository. Detection and scope-uncertainty handling remain open.


- Use fixed-version Unicode normalization and full case folding as a conservative component-wise comparison during configuration validation and migration prechecks; reject equality and ancestor/descendant overlap, preserve names, and do not claim this covers every filesystem alias. Exact normalization algorithm, Unicode version and tests remain open.
- When inspection required to establish migration safety cannot read or interpret directory entries, dry-run reports uncertainty and execution refuses with a reason; failed checks are not evidence of no conflict. Unrelated unreadable directories do not automatically block all operations. Exact inspection scope and error shape remain open.
- Storage migration targets must not exist, even as empty directories. This supersedes the prior prohibition limited to non-empty targets. Do not remove or replace existing targets; prevent an external target created after prevalidation from being overwritten, and refuse if this cannot be guaranteed. Mechanism and feasibility remain open.
- If Git exists but project scope within its containing working tree cannot be determined reliably, history/diff fail explicitly instead of guessing, broadening the query or reporting a normal empty result. Confirmed missing Git or no repository retains informational empty results.


- String comparison is not enough to establish filesystem safety. Inspect relevant existing objects for aliases that could cause category overlap, migration overwrite or escape; report conflicts and refuse the affected operation. Migration inspection uncertainty follows the existing dry-run/execution rule. Mechanism and verifiable scope remain open.
- During storage-migration rollback, do not overwrite a new object at an original path. If that move cannot safely be restored, report rollback failure, affected paths and the actual location of any remaining recovery material; never delete or overwrite the external object for apparent success. Malicious ancestor replacement remains outside the guarantee.
- A supported ordinary-text untracked item diff is a Git new-file patch using working-tree-relative paths and applicable to an appropriate clean baseline. Human CLI stdout is patch only; CLI JSON and MCP carry identical patch content. Report failure rather than emit a fake patch if safety cannot be established. Cross-platform verification and Git configuration effects remain open.


- Work-item create must not overwrite an existing or concurrently created target. Report conflict and leave the external object untouched; prechecking and the cooperative lock alone are insufficient, and create refuses if the guarantee is unavailable.
- Reject writes to existing work-item targets that are any Windows reparse point or whose type cannot be established. Never write through them; a separate path precheck alone is insufficient. Native mechanism and read policy remain open; project-root aliases remain allowed.
- On failed storage migration, only attempt to remove missing-parent directories actually created by this run, only while still empty, without recursion or deleting external content. Report cleanup failure or uncertain state; dry-run never creates directories. Safe identification under races remains open.


- Use Unicode 16.0.0 canonical caseless comparison, `NFD(full_default_case_fold(NFD(component)))`, component by component for equality and ancestry; preserve original names. Verify dependency/data consistency, Rust 1.85 and conformance; Unicode table upgrades are explicit compatibility changes. This does not cover all filesystem aliases.
- Standalone `.` is permitted for storage-root configuration and corresponding migration input and denotes the Waymark project root, not the Git repository root. With `.` the tracked-file storage-root diff can include other project files, and is documented accordingly; nested project scope remains bounded.
- Standalone `.` is forbidden for category paths and category migration targets. Existing configurations are diagnosed and remediated under the earlier transition policy; no automatic renaming.
- Apart from standalone storage-root `.`, reject `.` path components such as `./task` and `work/./task` rather than silently normalize. This applies to persisted storage/category paths and migration targets; diagnose existing incompatible configurations.


- The migration no-existing-target rule applies to actual category move destinations, not to an already existing storage-root container or target parent. Migration to `.` may move `work/task` to absent `task` without moving the project root; reject if an actual category destination is the existing project root.
- Category paths may be outside the storage root. Default Git history/diff scoped to the root may omit them; individual IDs remain queryable. A root migration that cannot derive an out-of-root category destination refuses and names it rather than guessing.
- Unicode data upgrades must explicitly review compatibility: before affected writes validate under the newer comparison, report collisions, retain configuration diagnosis and manual repair, and never silently merge or rename. Release notes tell collaborators to coordinate versions; exact version representation remains open.


- An untracked single-item patch represents actual working-tree file content when requested, not a predicted staged representation, and for supported ordinary text should reproduce it on a compatible clean target worktree. Do not promise byte identity after target-side conversion, filtering or special apply settings. Fail explicitly if a safe applicable patch cannot be produced.
- Do not invoke external diff or textconv for the applicable untracked-item patch; preserve Git path quoting. Fail rather than emit a fake patch when other attributes prevent safety. Supported Git attributes and platform/version tests remain open.
- Inventory/projection scans do not follow any Windows reparse point as a work-item document; diagnose the path and never claim an uncertain scan is complete. Independent valid scoped reads remain available; project-root aliases and adversarial ancestor limits are unchanged. Direct-read details and native verification remain open.


- Direct work-item reads refuse final symbolic links and all Windows reparse points, fail if target type cannot be established, and cannot rely on a separate pre-read path check alone. Other valid scoped reads and project-root aliases remain allowed; native mechanism is open.
- `git history --path` is a literal project-internal query path on all platforms; reject absolute, drive-relative and project-escaping forms, not all Windows-portability restrictions for persisted configuration. Special names and scope verification need testing.
- Remove a migration-created parent directory on failure only if it is still identifiable as the same directory this run created and is empty; otherwise leave it and report incomplete cleanup. Matching path alone is insufficient; implementation feasibility remains open.

## Still to resolve

- Exhaustive Windows reserved-name and character rules; Unicode table/dependency verification and native collision checks; detailed guidance for existing incompatible configurations.
- Path traversal, reparse points, project-root aliases, and collision checks in the context of existing lock and migration decisions.
- Git path arguments, discovery, untracked single-item edge cases, error behavior and native verification.
