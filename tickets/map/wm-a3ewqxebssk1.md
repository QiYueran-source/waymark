---
schema: 1
id: wm-a3ewqxebssk1
title: Define cross-platform file replacement and migration guarantees
category: map
status: open
owner: frank
created_at: 2026-11-05T00:53:09Z
updated_at: 2026-11-05T02:01:02Z
parent: wm-4wbvzsv34g2p
dependencies:
  - wm-78d0sa7jaxpb
---
## Type

grilling

## Question

What guarantees and explicit limitations should file replacement, rollback, permission handling, directory synchronization and same-filesystem storage migration provide on each supported local filesystem, including cross-volume preflight and dry-run?

## Decisions confirmed so far

- Confirmed by the user on 2026-11-05: retain the single-file atomic replacement contract on supported platforms, including Windows. A reader of an existing document sees either the complete old content or the complete new content, not an intermediate partial document. Do not silently fall back to in-place overwrite; a filesystem or implementation unable to provide the guarantee is not accepted as supporting it. Native validation and the choice of mechanism remain open.
- Do not promise automatic recovery of a multi-file operation after power loss, operating-system crash, or forced process termination; do not promise that every successful write is durable after power loss. For ordinary handled write errors, retain the existing application-level rollback requirement and explicit rollback-failure reporting. A per-file atomic replacement is not a multi-file filesystem transaction.
- A storage migration across filesystem volumes is rejected. It must not silently substitute copying and deleting for the same-filesystem directory move. Preflight behavior when volume identity is uncertain remains open.
- On Windows, replacing an existing file must not silently broaden its prior access permissions: e.g. a previously restricted work item must not become readable by additional users as a side effect of an edit. Windows ACLs are not Unix mode bits; exact preserved attributes and failure policy remain open.

- Confirmed by the user on 2026-11-05: when storage-migration preflight cannot determine whether source and target are on the same volume, dry-run explicitly reports the uncertainty and the actual migration refuses to execute. Actual execution repeats complete prevalidation; an earlier successful preview is not authorization. A successful precheck does not guarantee the eventual rename succeeds, so execution failures must still be handled and reported.
- On a handled file-replacement error, do not assume the failing destination was unchanged: inspect and handle its actual state as well as restoring earlier completed targets. If restoration fails or the final state cannot be confirmed, explicitly report affected paths and recovery state instead of returning an ordinary failure that implies nothing changed.
- On Windows, if Waymark cannot establish that replacement preserves the existing file's access restrictions without broadening them, refuse the replacement and report an error rather than inheriting potentially broader temporary-file or directory permissions. If a failure during replacement leaves final permissions uncertain, report affected paths and recovery state under the preceding failure rule. ACL mechanism and exact metadata coverage remain to be determined.
- If an existing work-item file is marked read-only, editing fails and preserves the original file; Waymark does not automatically clear the read-only attribute in order to replace it. Windows read-only attributes and ACLs are distinct and both require validation and tests.

## Still to decide

Specify exact Windows security metadata preservation, recovery reporting details, filesystem capabilities and native verification.
