---
schema: 1
id: wm-ahdybfq7rwkv
title: Define configuration and frontmatter evolution and migration policy
category: map
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:21Z
updated_at: 2026-11-03T23:52:23Z
closed_at: 2026-11-03T23:52:21Z
parent: wm-y14hs95k8wct
dependencies:
  - wm-wr11be4s3vpd
---
## Resolution

- Keep a single `config_version` for project configuration and work-item frontmatter; remove the separate `frontmatter_version`. Waymark commands own version metadata and users do not edit it manually.
- `config_version` identifies format/semantic compatibility, not ordinary data revisions. It advances when an incompatible format/semantic change is introduced, not on every routine write.
- Older-version files may be read using known compatibility rules. A normal write may lazily upgrade that file to the current version. Files declaring a version newer than the running Waymark are rejected with a structured, locatable error and are never guessed at or downgraded.
- Incompatible migrations are explicit (`waymark migrate`), not implicit on read. Migration supports dry-run and reports affected files, summaries, and warnings.
- Before changing files, migration validates the complete target set. Invalid, unparseable, or non-losslessly mappable data fails the migration with per-file/field diagnostics; it does not silently discard, guess, or repair data.
- Backups are stored under a dedicated directory inside `.waymark/` (e.g. `.waymark/backups/`). Existing backups are never silently overwritten, and backup failure prevents migration writes.
- Migration stages and validates all outputs before replacing targets, uses same-directory temporary files and atomic single-file replacement, and rolls back already-replaced files on failure. It must not leave a partial migration; retain recovery material and report clearly if rollback cannot complete.
- Serialization is deterministic and stable; preserve unrelated content where possible, and avoid meaningless diffs or changes on repeated/current-version migrations.
- Configuration is managed through Waymark commands; manual edits are unsupported. Missing/invalid configuration or version metadata is diagnosed, not silently defaulted, recreated, or overwritten.
- The archive data directory should also be under `.waymark/`, separate from backups. The archive lifecycle ticket already defines archive semantics; this location decision does not change those semantics.
