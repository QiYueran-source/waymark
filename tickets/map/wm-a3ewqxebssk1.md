---
schema: 1
id: wm-a3ewqxebssk1
title: Define cross-platform file replacement and migration guarantees
category: map
status: open
owner: frank
created_at: 2026-11-05T00:53:09Z
updated_at: 2026-11-05T02:06:10Z
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

- Confirmed by the user on 2026-11-05: when editing an existing Windows file, preserve its owner and discretionary access control list (DACL), including inheritance/protection state; refuse replacement if preservation cannot be established. This requirement does not decide preservation of system access control lists (SACLs). Native feasibility, required privileges and interaction with the no-broader-access requirement must be verified.
- For an existing restricted target, protect temporary files throughout creation, staging, replacement and cleanup so they are not accessible more broadly than the target. If this cannot be ensured, refuse the write. This rule applies to the entire staging interval, not only the final file; its mechanism and native verification remain open.
- A batch recovery error must distinguish paths confirmed restored from paths whose restoration failed or whose final state is unknown; identify the failing path and locations of any recovery material that actually exists. It must not claim success or imply that nothing changed. Exact machine-readable fields remain for later interface design.
- Native Windows verification must cover replacement of a restricted-ACL file, refusal to edit a read-only file, temporary-file access restrictions, replacement failure and rollback reporting, and refusal of cross-volume migration. CI provisioning and the policy for unavailable test capabilities remain for the verification and release decision.

- Confirmed by the user on 2026-11-05: any backup or recovery file containing old restricted content must be no more broadly accessible than its original throughout creation, use and retention. If recovery material cannot be safely protected, do not begin an operation that relies on it for recovery. This does not promise durable crash recovery or that recovery files always exist.
- For a newly created work item with no existing target ACL, the final file may inherit its destination directory's default permissions; do not guess an unrelated work item as a permission template. Staging a new file must not grant broader access than the intended final directory policy. How to enforce this on each supported platform requires native verification.
- No general preservation of additional Windows metadata such as SACLs, alternate data streams or timestamps is promised at this stage; this does not relax the agreed owner, DACL, read-only, access-control or data-safety guarantees. A concrete need or incompatibility can be brought back for a separate decision.
- If abrupt termination leaves temporary files or recovery materials, later diagnostics should notify users to inspect those leftovers. Do not automatically treat them as authoritative, restore them, or overwrite existing work items. Identification and safe diagnostic behavior remain to be designed; no automatic crash recovery is promised.

- Confirmed by the user on 2026-11-05: diagnostics must not infer that a temporary or recovery file is abandoned solely from its name or age. If activity cannot be ruled out, describe it only as a file requiring inspection and do not claim it is safe to delete; another Waymark process might be using it.
- Diagnostics of potential temporary or recovery material report its path and the reason for inspection; they do not automatically delete, move, restore, or follow a link to read its contents. This does not make the material authoritative or promise automatic crash recovery.
- A local filesystem is supported only if it meets the agreed locking, atomic replacement, permissions, and migration requirements; local status alone does not prove those guarantees. Do not silently weaken safety when capabilities cannot be established. The exact supported filesystem list and verification strategy remain for the verification and release decision.
- Do not describe an incomplete or failed file/directory synchronization as safely persisted to disk. Atomic visibility is distinct from power-loss durability. Which sync failures make an operation fail remains open for platform implementation and testing; no durable-commit guarantee is implied.

## Still to decide

Review the overall contract with the user. Exact sync failure behavior, supported filesystem lists, diagnostic identification, implementation feasibility and verification remain for platform design and verification decisions.
