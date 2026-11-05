---
schema: 1
id: wm-r6bmw1xrk5mn
title: Define platform response interface details
category: map
status: closed
owner: frank
created_at: 2026-11-05T05:38:01Z
updated_at: 2026-11-05T06:44:32Z
closed_at: 2026-11-05T06:44:32Z
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

## Final interface decisions (reopened to close the TBDs)

- Confirmed by the user on 2026-11-05: `DIRECTORY_SYNC_UNCONFIRMED` details carry `{directory, paths[]}`, where both use the shared path representation; the warning leaves `location` unset, because one warning covers several paths and a single location would mislead the caller.
- The tagged path representation applies uniformly to every machine path field, including path members inside structured locations, rather than only to fields that might plausibly be non-UTF-8. Consumers must not have to decide field by field whether a destination can be an object; a UTF-8 path is still a plain string, and only a non-representable path becomes the base64 object.
- Human-readable output renders bytes that cannot be decoded as `\xNN`, leaving the rest of the path readable, with no ellipsis, no guessing and no `�` substitution.
- Waymark's own created paths are always valid UTF-8; a non-UTF-8 path can only be an existing external file or directory that is referenced. The specification says this explicitly, so lossless representation is a boundary case rather than a claim that the store handles arbitrary byte paths, and the store keeps using UTF-8 path types instead of migrating to byte paths.
- These close the remaining interface TBDs; the earlier remaining-open list is superseded where it overlapped.
