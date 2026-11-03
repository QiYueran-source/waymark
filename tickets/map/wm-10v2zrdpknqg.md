---
schema: 1
id: wm-10v2zrdpknqg
title: Define file concurrency and in-process projection behavior
category: map
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:20Z
updated_at: 2026-11-03T23:52:21Z
closed_at: 2026-11-03T23:52:20Z
parent: wm-y14hs95k8wct
dependencies:
  - wm-2qd0yahgpa8t
  - wm-wr11be4s3vpd
---
## Resolution

### Authoritative files and in-process static projection

- Work-item Markdown files remain the sole authoritative data. Waymark v1 does not persist a JSON index. The shared domain library may retain a complete in-process projection of parsed frontmatter, source paths, fingerprints, diagnostics, and forward/reverse core relationships; it is disposable and rebuilt from files.
- Derived values are computed from the current projection and are never persisted. Local reads may use scoped temporary frontmatter parses without loading entries into the long-lived projection; global queries and cross-file writes require a complete fresh projection.
- Long-lived contexts perform lightweight inventory/freshness checks before reuse. New, deleted, moved, changed, or uncertain files invalidate the relevant projection or whole projection; parse errors allow valid local reads but block complete graph operations.
- Projection updates occur after authoritative writes. Incremental synchronization is allowed only when its affected scope is known; otherwise rebuild the complete projection. A bounded LRU may cache body/details/query data but cannot evict relation nodes or bypass freshness.

### Concurrency and batch writes

### Concurrency and batch writes

- Waymark writes use optimistic conflict detection against the content fingerprint read by the operation. If the target changed, reject the write and require reread/retry rather than overwriting another writer.
- A project-level write lock coordinates Waymark processes and protects conflict-check-plus-replacement from races. External editors that do not honor the lock are handled through fingerprint conflict detection.
- Batch writes prevalidate and stage all targets under the lock. They provide application-level all-or-nothing behavior: on failure, roll back already replaced files and retain recovery material. This is not a claim that the filesystem provides a true multi-file atomic operation; if rollback cannot complete, report the partial-write/recovery state explicitly and never claim success.
- Authoritative files and derived index are not one transaction: an index failure does not roll back successfully written work-item files.
