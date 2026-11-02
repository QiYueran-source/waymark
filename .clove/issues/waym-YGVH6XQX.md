---
schema: 1
id: waym-YGVH6XQX
title: Define configuration and frontmatter evolution and migration policy
status: closed
type: feature
priority: 2
created: 2026-11-02T06:14:40.958Z
updated: 2026-11-02T07:46:55.995Z
closed: 2026-11-02T07:46:55.995Z
parent: waym-VS5Z5543
labels: [wayfinder:grilling]
deps: [waym-0Y4VRHNA]
relates: []
duplicates: []
supersedes: []
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