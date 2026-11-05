---
schema: 1
id: wm-r6bmw1xrk5mn
title: Define platform response interface details
category: map
status: closed
owner: frank
created_at: 2026-11-05T05:38:01Z
updated_at: 2026-11-05T05:40:43Z
closed_at: 2026-11-05T05:40:43Z
parent: wm-4wbvzsv34g2p
dependencies:
  - wm-rn893kzbg524
---
## Type

grilling

## Question

Fix the response shapes left open by the platform contracts: the registry code and details schema for directory-synchronization warnings, how the three recovery states map onto the existing applied/failed/rollback_failed detail lists, and the lossless representation of paths that cannot be encoded as UTF-8.

## Decisions confirmed so far

- Confirmed by the user on 2026-11-05: the directory-synchronization warning is carried by a new registry code `DIRECTORY_SYNC_UNCONFIRMED` with no exit class and warning severity, and by extending the shared `Warning` type with an optional `details` field. Details follow the same discipline as error details: a `serde_json::Value` whose permitted keys are registered per code, pinned by the same golden snapshot that pins error details, so warnings do not acquire a second mechanism. Warnings remain in the success `_meta.warnings` collection and never change the success status or the CLI exit code.
- Confirmed by the user on 2026-11-05: the `ROLLBACK_FAILED` details replace the ambiguous `applied`, `failed` and `rollback_failed` lists with explicit three-state reporting: `restored`, `failed` and `unknown`, plus `recovery_material` entries carrying a path and the reason it requires inspection. Each affected path appears in exactly one of the three states. The old lists are removed rather than kept alongside the new ones, because Waymark is an unreleased local tool with no external consumer to preserve, and keeping both would let "unknown" be misread again. The specification is rewritten at synchronization time; two shapes never coexist in one released contract.
- Confirmed by the user on 2026-11-05: a path in a machine response is represented as a tagged union. A path that is valid UTF-8 stays a plain string, exactly as today. A path that cannot be represented losslessly becomes an object carrying base64 bytes. Human-readable output escapes the same bytes rather than substituting replacement characters. Base64 for every path and percent-escaping raw bytes were both rejected: the first makes all ordinary output unreadable, the second requires every consumer to implement a decoder that is easily confused with a real filename.
- These decisions fix shapes, not implementations. No response in the current code carries `details` on a warning, the recovery details still use the old lists, and path fields are plain strings. Implementation and native verification remain separate work.

## Remaining open

- Which exact keys `DIRECTORY_SYNC_UNCONFIRMED` details carry, and whether the directory itself is also expressed through `location` or only within details.
- Whether the path tagged union applies to every machine path field or only to fields that can genuinely receive a non-UTF-8 value, and how the tagged form interacts with fields that are already structured locations rather than bare paths.
- Whether the human-mode escape format for non-UTF-8 bytes is a single documented form across CLI messages, diffs and diagnostics.
- How existing on-disk paths that are not UTF-8 are even reached, given that `camino::Utf8Path` is used throughout the store; that reachability question belongs to the platform module and verification work.
