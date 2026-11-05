---
schema: 1
id: wm-bpz94xwr28qa
title: Reconcile cross-platform decisions with existing specifications
category: map
status: closed
owner: frank
created_at: 2026-11-05T00:53:10Z
updated_at: 2026-11-05T06:37:27Z
closed_at: 2026-11-05T06:37:27Z
parent: wm-4wbvzsv34g2p
dependencies:
  - wm-9ghaw0aqapyf
  - wm-dq6f226w3g5k
  - wm-pkn47knm5t0n
  - wm-rn893kzbg524
---
## Type

grilling

## Question

How should the agreed platform contracts and release scope reconcile with the existing Unix-only functional specification, Windows specification and release design, without creating competing normative specifications? Define the handoff and explicit synchronization boundary.

## Decisions confirmed so far

- Confirmed by the user on 2026-11-05: one normative document survives. The functional specification becomes platform-neutral in its body with explicit per-platform statements where behaviour actually differs, and the Windows platform specification is absorbed into it and closed. Its old text remains in Git history only. Two documents would be two sources of truth, and the conflicts found during this work were exactly of that kind: the same rule — locking, replacement — stated differently in two normative places. A permanent platform-delta appendix was rejected for the same reason: it leaves every reader asking which document wins.
- Confirmed by the user on 2026-11-05: the specification is synchronized before implementation tickets are cut. All platform decisions currently live only in the design maps, and the existing specification contradicts several of them — it states Unix-only scope, MSVC-only Windows support, a check-then-open symlink race treated as acceptable, no Windows binding, and GNU out of scope. Cutting tickets first would produce tickets written against superseded text.
- Confirmed by the user on 2026-11-05: superseded passages are rewritten in place. No errata layer, no "the following clauses override the above" patch section. The replaced text lives in Git history; a reader must not need to know how many earlier versions of a rule existed.
- Confirmed by the user on 2026-11-05: the frozen error registry is inside the reconciliation scope. The agreed changes — `ROLLBACK_FAILED` reporting `restored`, `failed` and `unknown` instead of the ambiguous applied/failed/rollback_failed lists, the new `DIRECTORY_SYNC_UNCONFIRMED` warning code, and the two read-only diagnostic codes for material requiring inspection and for unconfirmed coverage — require the registry table, the golden snapshot, the JSON schema and the compile-time match to change in the same commit, or the specification and the implementation diverge while both look correct. The release specification is updated in the same effort to add the two Windows CI legs and the MSVC-only artifact.

## Synchronization boundary

- Synchronization updates normative text only. It does not implement the platform module, change the CI matrix, or add release jobs; those remain implementation work cut from the synchronized specification.
- The Windows-only behaviour that must appear as explicit per-platform statements rather than as platform-neutral prose: capability declarations, owner and DACL preservation, read-only refusal, reparse-point refusal, NTFS-only support, and the MSVC-only artifact with a GNU build-and-test leg.
- Claims that stay out of the specification until evidence exists: any statement that Windows is supported, that a capability has been verified, or that a release artifact has been produced. The specification states required behaviour and required evidence, not achieved results.
- Linux and macOS behaviour already covered by the current specification is preserved unless a platform decision explicitly changed it; the synchronization is not an occasion to restate or re-derive settled non-platform rules.

## Resolution

The user confirmed the reconciliation approach on 2026-11-05. This closes the decision about how to reconcile; it does not perform the reconciliation. No specification text has been changed, no implementation ticket has been cut, and the Windows specification deliberately still contains the superseded statements listed above.

## Handoff

The synchronization itself is a separate authorized effort: rewrite the functional specification, absorb and close the Windows specification, update the release specification, and change the registry table with its golden snapshot, schema and compile-time match in one commit. Implementation tickets are cut from the synchronized specification afterwards, in dependency order: compilation repair and the platform module skeleton first, then lock and replacement primitives, path grammar consolidation, migration, Git, and finally warning and diagnostic surfaces. Native Windows evidence follows implementation and is recorded as evidence rather than assumed.
