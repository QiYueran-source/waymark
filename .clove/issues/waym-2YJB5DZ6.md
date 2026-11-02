---
schema: 1
id: waym-2YJB5DZ6
title: Define file concurrency and optional SQLite index behavior
status: closed
type: feature
priority: 2
created: 2026-11-02T06:14:40.996Z
updated: 2026-11-02T07:55:59.457Z
closed: 2026-11-02T07:55:59.457Z
parent: waym-VS5Z5543
labels: [wayfinder:grilling]
deps: [waym-0Y4VRHNA, waym-HDBFBM0Y]
relates: []
duplicates: []
supersedes: []
---
## Resolution

### Authoritative files and derived JSON index

- Work-item files remain the sole authoritative data. The Git-tracked index is a deterministic JSON artifact at `.waymark/index.json`; it is derived and never used to repair or overwrite work-item files.
- The index has its own format version, independent of `config_version`, and includes enough source-file inventory/fingerprint information to detect missing, changed, or uncertain inputs. Stable ordering and deterministic serialization ensure identical source state produces identical index bytes.
- Missing, corrupt, or unsupported-version indexes are rebuilt from authoritative files. Provide an explicit rebuild operation; build and validate a replacement before atomically switching it in. Diagnostics can compare the index against a full file scan and locate discrepancies.
- Before an indexed query, validate freshness. If any source is missing, changed, or freshness is uncertain, scan authoritative files and rebuild; never silently return known-stale results. Queries and writes remain possible by scanning files if index use is unavailable.
- After authoritative writes succeed, update and atomically replace the index under the project write lock. If index update fails, keep the successful authoritative write, report that the index needs rebuilding, and allow later query/rebuild to recover it.
- Do not manually merge conflicting index JSON. After Git merges the authoritative files, rebuild the index from those files and commit the regenerated artifact.

### Concurrency and batch writes

- Waymark writes use optimistic conflict detection against the content fingerprint read by the operation. If the target changed, reject the write and require reread/retry rather than overwriting another writer.
- A project-level write lock coordinates Waymark processes and protects conflict-check-plus-replacement from races. External editors that do not honor the lock are handled through fingerprint conflict detection.
- Batch writes prevalidate and stage all targets under the lock. They provide application-level all-or-nothing behavior: on failure, roll back already replaced files and retain recovery material. This is not a claim that the filesystem provides a true multi-file atomic operation; if rollback cannot complete, report the partial-write/recovery state explicitly and never claim success.
- Authoritative files and derived index are not one transaction: an index failure does not roll back successfully written work-item files.