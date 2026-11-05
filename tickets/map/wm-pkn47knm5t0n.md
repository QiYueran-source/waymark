---
schema: 1
id: wm-pkn47knm5t0n
title: Define filesystem warning and recovery response contract
category: map
status: closed
owner: frank
created_at: 2026-11-05T02:09:09Z
updated_at: 2026-11-05T04:39:03Z
closed_at: 2026-11-05T04:39:03Z
parent: wm-4wbvzsv34g2p
dependencies:
  - wm-a3ewqxebssk1
---
## Type

grilling

## Question

How do CLI, MCP and shared machine responses represent a successful replacement whose directory synchronization failed, aggregate warnings for multi-file operations, and report failing, restored, unknown and recovery-material paths without weakening the existing error and exit contracts?

## Decisions confirmed so far

- A successful replacement whose directory synchronization failed produces one warning per failed synchronization event, not one per affected file and not one aggregate count. The warning identifies the directory and the affected paths in addition to its human-readable message, because a single event can cover several files. Warnings appear in the success `_meta.warnings` collection, never change success status, and never change the CLI exit code.
- The warning shape gains a per-code optional details field alongside the existing `{code, message, location}` entry instead of duplicating an identical warning per path. The code is a new registry entry with no exit class and warning severity; its exact name and details schema belong to interface design, as does the lossless representation of paths that cannot be encoded as UTF-8.
- A warning never becomes a durability claim. A successful replacement with an unconfirmed directory synchronization must not be worded as durable after power loss.
- Inability to attempt directory synchronization is distinct from a failed attempt, and only a failed attempt warns. A platform without a directory-entry durability mechanism declares that as a documented limitation once, with verification evidence, instead of warning on every successful write; the operation is still never described as durably persisted. Native tests must show no fabricated warning on a platform that succeeds and one warning on a failing attempt.
- Every affected path in a failed multi-file recovery is classified into exactly one of confirmed restored, not restored with a known final state, or final state unknown. "Unknown" may not be folded into either other state. The response identifies locations of recovery material that actually exists and why it requires inspection. Exit class 7, `ROLLBACK_FAILED` and non-retryable classification are unchanged.
- Human-mode CLI output must list the affected paths and their states rather than only counts; JSON and MCP carry the same content structurally.

## Remaining open

- Registry code name and details schema for the directory-synchronization warning; whether the optional details field is added to the shared warning type or to a success-only companion structure.
- Which of the three recovery states, if any, corresponds to the existing `applied` and `failed` detail lists, and how existing consumers migrate.
- Lossless non-UTF-8 path representation inside warnings and recovery details.
- Native Windows evidence for the declared directory-synchronization capability, and whether MSVC and GNU share one declaration.

## Resolution

The user confirmed the response contract above on 2026-11-05. It records behavior, not an implemented or verified state: the current engine ignores directory-synchronization errors, the shared warning entry has no details field, and human failure output prints counts rather than paths, so all three must change before the contract holds.

## Handoff

Exact interface work remains and is routed through the platform module design: the registry code name and warning details schema, whether optional details extend the shared warning type or a success-only structure, how the three recovery states relate to the existing `applied`, `failed` and `rollback_failed` detail lists, and the lossless non-UTF-8 path representation. Native Windows evidence for the declared directory-synchronization capability belongs to verification and release gates. Do not create implementation tickets or synchronize the specification from these decisions yet.
