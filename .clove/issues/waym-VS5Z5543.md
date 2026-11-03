---
schema: 1
id: waym-VS5Z5543
title: 'Waymark: General-purpose tracker design map'
status: open
type: epic
priority: 2
created: 2026-11-02T06:14:23.060Z
updated: 2026-11-02T06:14:23.060Z
labels: [wayfinder:map]
deps: []
relates: []
duplicates: []
supersedes: []
---
## Destination

Produce a handoff-ready functional specification for Waymark, a general-purpose, Git-friendly work-item tracker with configurable storage and project-level custom fields. This map resolves remaining product and domain decisions; it does not implement Waymark.

## Notes

- Use Clove as this repository's issue tracker.
- Treat Clove as a behavioral reference, not a required feature checklist.
- Keep categories and project workflow conventions configurable; do not build Wayfinder-specific semantics into Waymark's core.
- Consult the grilling and domain-modeling skills for decision tickets.

## Decisions so far

- [Define Waymark command and integration surface](waym-68AB4C9B): Core domain operations live in a shared function library; CLI and MCP are adapters, frontier-specific semantics stay outside the generic tracker, and general leaf/tree queries are in scope.
- [Define issue lifecycle, archive/delete, and reference integrity](waym-B85RM1BH): Archive is irreversible through Waymark and acts as logical deletion; the original file remains, archive metadata is added in frontmatter, active children block archiving, and Waymark clears recognized inbound core relationships in one safe batch without touching body/custom-field references.
- [Define owner transfer, operator guardrails, and write authorization semantics](waym-TNAYVX4S): Create may assign another human owner; every later mediated write checks declared operator against owner, transfers take effect immediately, and batches are all-or-nothing; identities are not authenticated.
- [Define project-level custom field types and schema syntax](waym-0Y4VRHNA): Custom fields use basic shape types only (string, integer, boolean, homogeneous list); enum is a string convention, fields are optional by default, and custom values have no reference/workflow behavior.
- [Define generic custom-field filtering and projection semantics](waym-HDBFBM0Y): A finite generic query set supports presence, equality, typed comparisons and string matching; list membership is explicit; null is disallowed; sorting and projection have deterministic missing-value behavior, with strict type errors.
- [Define configuration and frontmatter evolution and migration policy](waym-YGVH6XQX): One command-managed `config_version` covers config and frontmatter; incompatible migrations are explicit, dry-runnable, prevalidated, backed up under `.waymark/`, and atomic; older files can be lazily upgraded on write, future-version or invalid data is rejected, serialization is deterministic, archive metadata stays in work-item frontmatter, and migration backups stay under `.waymark/`.
- [Define file concurrency and index behavior](waym-2YJB5DZ6): Work-item files are the sole authority; Waymark does not persist a JSON index. A complete in-process static frontmatter projection is rebuilt or incrementally synchronized from files, with explicit completeness/freshness states, project-level locking, fingerprint conflict checks, and application-level batch rollback.
- [Define read-only Web tree overview and navigation](waym-CMBAPDH0): Web is a strictly read-only visual overview with combined/category-grouped trees, category shape cues, work-state colors, optional dashed dependency overlays, simple details/navigation, and browser-local view-state persistence; archived items are excluded by default.
- [Define Waymark delivery scope, archiving UI, and history capabilities](waym-J12VZ4AX): No separate archive browser, no delivery-phase commitments, no import/export or separate audit log; Git provides history/diff, and the resolved tree overview is the complete Web scope. Explicit archive-aware CLI/MCP queries remain available; Web stays read-only and excludes archived items.
- Stable IDs are independent of category, name, and path; storage root and category paths are configurable. Configuration includes a project time zone for human-readable time output and parsing local time inputs without offsets; persisted timestamps remain UTC RFC 3339, offset-bearing inputs retain their instant, and changing the time zone never rewrites work-item files. Ordinary operations cannot accept caller-forged system timestamps; a future explicit historical migration/import path may convert legacy times.
- Categories are project-defined groupings, without category-specific field schemas in the initial model.
- Parent is a single cross-category hierarchy edge; parent and hard dependencies are independent, and only dependencies block readiness. Parent lifecycle does not cascade to children.
- Core fields have stable meanings. Project custom fields are declared at project scope, uniquely named, optional by default, and do not implicitly alter core behavior. Executor is project-defined rather than a Waymark core field; generic field querying must be available.
- Creator is provenance; owner is required human accountability. Only the current owner may initiate an owner transfer, and owners must be human.
- The configured operator/owner check is a process guardrail, not authentication. Executor metadata does not grant write authorization.
- Clove's strict, predictable frontmatter is a format reference, but Clove itself has no custom-field schema/query support. Clove's unified edit path is the reference for full-body replacement, empty-body clearing, non-interactive `--body`/`--body-file` CLI modes, and structured MCP edits; Waymark v1 intentionally omits interactive editor support.
- Core-field decisions are resolved in `docs/superpower/WAYMARK-FUNCTIONAL-SPEC.md`: the complete persisted v1 field set, `closed_at`, ID/category-key formats, identity/time normalization, frontmatter archive metadata, and computed `blocked`/`ready`/`blocked_by` fields with dangling-dependency diagnostics. `blocked` is not a lifecycle status; `blocked_by` is a read-only computed field and is never persisted.
- Index/cache decisions are resolved: documents are the sole authority; a complete in-process static frontmatter projection is derived from them; derived readiness and graph results are computed dynamically; local operations may use incomplete projections only when explicitly marked, while global queries and archive require completeness. A bounded LRU may cache body/details only, never evict relation nodes.
- `touch` is an internal term for a Waymark-controlled frontmatter query, frontmatter mutation, or archive; it is not a timestamp update and does not mean arbitrary body access. Public interfaces use explicit read/write/archive/sync names.
### Resolved archive semantics

- Archive is an irreversible-through-Waymark logical deletion, not a lifecycle transition and not `closed`. It is allowed from `open`, `in_progress`, or `closed`; it does not change `status` or `closed_at`.
- Archive preserves the original Markdown file and all existing metadata, including body, custom fields, `parent`, `dependencies`, and `soft_links`. It adds `archived_at` and optional `archive_reason` in the same frontmatter, does not add `archived_by`, and does not update `updated_at`.
- Only the current owner may initiate archive, subject to the declared operator/owner process guardrail. Archive is a mediated write; archived items reject later edit, body, status, owner, transfer, and relationship writes.
- Re-archiving returns `ALREADY_ARCHIVED`, does not replace `archived_at` or `archive_reason`, and does not create another change.
- An item with any unarchived child is rejected with `HAS_ACTIVE_CHILDREN`. Archive never cascades to children; users must archive children individually, clear their parent, or move them first.
- Archive does not provide a Waymark restore/unarchive command. A user may manually restore a historical Git version; current frontmatter determines whether the item is active or archived.
- An archived item is excluded from active tree, list, search, ready, blocked, and stats results. `show <id>` can read it. `--archived` selects archived-only collections; it does not mix archived items back into the active graph.
- The archived item's own relationship fields remain as historical context and are shown as historical relations. They do not participate in active graph calculations. New relationships cannot target an archived item.
- Archive removes recognized inbound core relationships from active files: inbound `dependencies`, inbound `parent`, and inbound `soft_links`. It does not inspect or modify body mentions or custom-field values. Affected active files update `updated_at`; the archived target does not.
- Archive requires a complete fresh project projection, performs full prevalidation under the project lock, writes the target and affected files as one application-level batch, and publishes a new projection only after success. Fingerprint conflicts, incomplete projection, parse errors, or write failures prevent the operation; rollback failure is reported explicitly.
- `archive --dry-run` performs the same completeness, owner, child, relationship, and conflict checks, reports the target and every planned relationship removal, and writes nothing.
- Archive may affect items owned by other people; only the archived target's owner is checked. The result contains structured changed-item details. No independent archive/audit log is designed; Git diff/history remains the durable audit trail. Large batches require explicit resource protection and never silently truncate.

### Resolved projection, cache, and touch semantics

- Work-item Markdown files are the sole authority. Waymark v1 does not persist `.waymark/index.json` or another JSON index.
- A process may hold an in-memory static frontmatter projection containing parsed core/custom fields, source paths, fingerprints, parse diagnostics, and forward/reverse core relationship helpers. It is disposable and rebuilt from files; it is not a second source of truth.
- The projection does not persist `ready`, `blocked`, `blocked_by`, children, dependents, or relationship diagnostics as facts. These are dynamically computed from the current static projection.
- A complete fresh projection is required for global list/stats/tree/ready/blocked queries, Archive, and cross-file relationship writes. A scoped/local operation may read a file directly without loading it into the long-lived projection, but must identify itself as scoped/incomplete.
- `show <id>` may use a direct file read and may temporarily read related frontmatter without recursively loading the whole graph. A `load_into_memory` strategy controls whether a read updates the long-lived projection.
- Long-lived MCP/Web contexts may retain a projection. Each operation performs lightweight inventory checks; new, deleted, moved, or uncertain files invalidate the relevant entry or whole projection. CLI may build a projection for one invocation and discard it on exit.
- A bounded configurable LRU is allowed only for body, detail, or query-result performance caching. It is fingerprint-bound, cannot evict relationship nodes from a complete projection, and cannot weaken freshness, completeness, authorization, or archive rules.
- `touch` is an internal term for a Waymark-controlled frontmatter query, frontmatter mutation, or archive. It is not arbitrary body access and does not mean updating a timestamp. Public APIs use explicit read/write/archive/sync names.
- Waymark writes documents first under the project lock with optimistic fingerprint conflict detection, then synchronizes or rebuilds the projection. Incremental synchronization is allowed only when the affected scope is known; otherwise the complete projection is rebuilt. Partial projection state is never published as complete.

### Resolved body-editing semantics

- Body editing follows Clove's unified partial-edit contract: absent body input leaves the body unchanged; a present empty string clears it; a present non-empty string fully replaces it.
- v1 does not provide interactive `$EDITOR` editing. CLI supports non-interactive `--body TEXT` and `--body-file PATH` (mutually exclusive); MCP accepts structured body input and never launches an editor.
- Create and edit canonicalize a non-empty body to exactly one trailing newline without trimming leading or internal whitespace. Body content is not interpreted as a relationship language.
- Body and frontmatter changes are validated and written as one fingerprint-checked atomic document replacement. A real ordinary body change updates `updated_at`; an unchanged body produces no write. Archive remains the explicit exception and does not update `updated_at`.
- Body search reads body on demand and may use its own LRU. A stable ID appearing in body text is not a structured relationship and is not automatically diagnosed or rewritten.

### Resolved validation, diagnostics, and identity rules

- Stable IDs are immutable and project-unique: `wm-` plus 12 Crockford Base32 characters, persisted lowercase and accepted case-insensitively. Duplicate normalized IDs, case variants, ID/path mismatches, or multiple files for one ID are errors; projection must not choose one arbitrarily.
- `doctor` is diagnostic-only for business data. It may report source/fingerprint changes, new/deleted/moved files, duplicate IDs, parse/validation errors, missing or archived targets, cycles, path conflicts, and projection incompleteness, but does not delete relationships, modify body/custom fields, restore items, or infer user intent. Cache/projection rebuild is the only safe internal repair.
- Fingerprint validation uses path inventory, size, and mtime as cheap signals, then content fingerprint when needed. If metadata changes but content fingerprint is equal, the parsed projection may be retained after refreshing metadata; only content changes trigger semantic re-import.
- Local reads may succeed for valid files when another file is broken. Global graph queries, Archive, migration, and batch relationship writes require a complete projection and fail explicitly on parse errors or uncertainty.
- Machine metadata exposes source (`direct_file` or `memory_projection`), completeness (`scoped` or `complete`), freshness, and diagnostics so callers can distinguish local reads from complete graph results.

### Resolved query, relationship, and migration semantics

- Collection queries are active-only by default. `--archived` selects archived-only; there is no implicit active-plus-archived collection. `show <id>` is the explicit ID-read exception.
- Structured list/query uses frontmatter and custom fields and does not read body. `search` may read title, frontmatter, and body, uses a body cache if available, and follows the same active/archived range rules.
- Parent, dependency, and soft-link mutations use dedicated validated operations rather than arbitrary generic field patches. Multi-target requests prevalidate and commit atomically; self-links, duplicate edges, archived targets, invalid references, and cycles are rejected. Soft links remain symmetric.
- Parent and dependency cycles are rejected at write time; doctor diagnoses cycles introduced by external edits. Soft links are non-blocking and do not form lifecycle cycles.
- Category display-name changes do not move files. Category key changes and category storage-path changes require explicit migration/move operations with full prevalidation. Path overlap, target collision, and case-equivalent collision are diagnosed and never overwritten.
- Custom field keys use lowercase ASCII `snake_case`; core and computed names are reserved. Schema changes that can invalidate existing values require explicit migration; adding optional fields or changing display metadata can be direct. Migration never silently drops, guesses, or partially converts values.
- Query results use deterministic ordering, stable-ID tie breakers, and snapshot-local pagination. Missing custom-field sort values use the previously defined deterministic rule. Stats always states its active/archived range and projection completeness.
- Core frontmatter references are structured and managed by Waymark. Body text and custom-field values are not treated as references, even when they contain stable IDs.
- Creator is assigned at creation and is immutable provenance; owner is required, may initially differ from creator, and is the current accountability boundary. Owner transfer is initiated only by the current owner, takes effect immediately, updates `updated_at`, rejects empty/whitespace identities, and is forbidden for archived items. The previous owner loses mediated-write authority immediately.
- Status is exactly `open`, `in_progress`, or `closed`. Any explicit transition among these states is allowed for active items; status changes update `updated_at`, entering `closed` sets `closed_at`, leaving it clears `closed_at`, and repeating the same status changes nothing. `closed_at` is system-maintained; inconsistent external values are diagnostics. Archive never changes status or `closed_at`.
- Create generates stable ID, `created_at`, and `updated_at`; callers cannot provide them. Generic edit cannot change `id`, `creator`, or `created_at`; `closed_at` is maintained only by status transitions; archive metadata only by archive; relationships only by dedicated validated operations or archive cleanup. External violations are diagnosed, not silently repaired.
- Create may include initial parent, dependencies, and soft links. The complete request validates ID/category/targets, active target state, self-reference, duplicates, cycles, owner guardrails, and atomic persistence before writing; invalid initial relations fail the whole create.
- System timestamps are generated by Waymark rather than accepted from ordinary callers; production APIs do not permit user-forged `created_at`, `updated_at`, `closed_at`, or `archived_at`. The project configuration supports a declared time zone for human-readable rendering and interpretation of local time inputs without offsets, while persisted timestamps remain UTC RFC 3339. Offset-bearing inputs retain their instant; changing the configured time zone never rewrites work-item files.
- Every public operation checks configuration freshness. Storage-root, category-key/path, and custom-field-schema changes invalidate the current projection and require rediscovery/validation; operator changes immediately affect mediated-write checks; cache-only changes clear evictable LRU data without changing the static projection.
- Stable-ID filenames use the persisted lowercase ID plus a fixed Markdown extension. Title/display names never enter filenames; category path/key changes move files without changing IDs. Existing targets, case-equivalent collisions, path overlap, and ID/path mismatch are errors, never overwritten or title-inferred.
- Git checkout/revert/reset is interpreted entirely from current authoritative files. Fingerprint changes invalidate old projections; no hidden restore log or automatic repair of other files is maintained. Reintroduced archived targets or old relationships are diagnosed from current content.
- Unsupported future format/config versions can be reported for local reads while leaving other valid files locally readable, but make the project projection incomplete and prohibit migration, archive, global statistics, and cross-file relationship writes. Waymark never guesses, downgrades, discards unknown data, or silently repairs it.
- The project config declares a time zone for human-readable output and interpretation of time input that lacks an offset. Persisted `created_at`, `updated_at`, `closed_at`, and `archived_at` remain UTC RFC 3339 values; changing the configured time zone changes display/input interpretation only and does not rewrite work-item files or update business timestamps. Machine responses default to UTC and may include configured-zone display values where defined. Inputs that already carry an offset are not rewritten according to config timezone.
- Ordinary create/edit/status/archive operations generate system timestamps from Waymark's clock and do not accept caller-supplied system times. A separately specified historical migration/import path may transform legacy times, but ordinary user APIs cannot forge `created_at`, `updated_at`, `closed_at`, or `archived_at`; external invalid time values are diagnostics.

## Not yet specified

No unresolved product decision remains for producing implementation tickets. The archive, in-process projection/cache, body-editing, projection validation, query-range, migration, relationship-operation, lifecycle/ownership, creation, and timezone contracts resolved so far are recorded above and must be reflected in the functional specification and implementation tickets. The design is ready for a consistency pass and then synchronization to the functional specification and implementation tickets when requested.

## Out of scope

- Implementing Waymark in this map.
- Wayfinder-specific map/frontier workflow semantics in the Waymark core.
- Agent claim/assignment prompting conventions; those belong to the relevant Agent skill or project workflow.
- Separate Web archive browsing, import/export, and a standalone application audit log; these are not part of this specification's scope.