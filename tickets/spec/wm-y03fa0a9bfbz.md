---
schema: 1
id: wm-y03fa0a9bfbz
title: Waymark general-purpose tracker functional specification
category: spec
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:20Z
updated_at: 2026-11-03T23:52:20Z
closed_at: 2026-11-03T23:52:20Z
---
# Waymark Functional Specification

## Problem Statement

Teams need a general-purpose work-item tracker that fits naturally into a Git repository, remains understandable and reviewable as ordinary files, and can support project-specific categories and metadata without imposing a particular team's workflow. They need to inspect, query, and update work consistently from command-line and agent interfaces while preserving explicit ownership, relationships, validation, and safe concurrent edits. A simple read-only Web view should help people understand the work hierarchy and progress at a glance.

## Solution

Waymark is a local-first, Git-friendly tracker for project-defined work items. Individual Markdown files with YAML frontmatter are the authoritative records; configurable project settings define categories, storage layout, and custom fields. A shared domain library supplies the semantics used by CLI and MCP adapters. The product provides predictable lifecycle and relationship behavior, structured machine-facing results and errors, safe validation/migration and writes, generic tree/leaf and custom-field query capabilities, and a strictly read-only Web tree overview.

The spec defines product capabilities and behavior, not delivery phases or a release plan.

## User Stories

1. As a project maintainer, I want to initialize a Waymark project with explicit configuration, so that the tracker knows where and how to store work items.
2. As a project maintainer, I want to configure a storage root and category paths, so that Waymark fits the repository's existing organization.
3. As a project maintainer, I want to define project-specific categories and display names, so that work can be grouped without adopting Waymark-specific workflow categories.
4. As a project maintainer, I want to configure project-wide custom-field schemas, so that items can carry metadata relevant to my team.
5. As a contributor, I want every work item to have a stable unique ID independent of its name, category, and path, so that references remain reliable when it is renamed or moved.
6. As a contributor, I want each work item stored as readable Markdown with typed YAML metadata, so that Git can review, diff, and merge ordinary project data.
7. As a contributor, I want to create a work item with a title, category, owner, and optional metadata, so that accountability is explicit from the start.
8. As a contributor, I want to read an item by stable ID and see its fields, body, and relationships, so that I can understand its current context.
9. As an owner, I want to edit an item while retaining stable identity and provenance, so that its history remains understandable.
10. As an owner, I want to move or rename an item without changing its stable ID, so that links do not break.
11. As a contributor, I want to set an item to open, in progress, or closed, so that its lifecycle is explicit and consistent.
12. As a contributor, I want closed items to satisfy hard dependencies, so that downstream readiness reflects completion.
13. As a contributor, I want to archive an item reversibly without changing its status or relationships, so that retired work leaves ordinary active views but remains recoverable.
14. As a contributor, I want to restore an archived item, so that previously retired work can return without losing identity or links.
15. As a contributor, I want archive state not to silently satisfy or invalidate dependencies, so that dependency meaning remains tied to completion status.
16. As a project maintainer, I want ordinary operations to avoid physically deleting referenced items, so that parent, dependency, soft-link, and custom references are not silently broken.
17. As a contributor, I want to set one parent independently of category and dependencies, so that hierarchy expresses containment without implying execution order.
18. As a contributor, I want to add hard dependencies separately from parentage, so that only explicit prerequisites affect readiness.
19. As a contributor, I want to add non-blocking soft links, so that related work can be connected without changing readiness.
20. As a contributor, I want generic tree and leaf queries, so that I can inspect hierarchy without adopting a particular workflow's notion of a frontier.
21. As a contributor, I want readiness to account for unfinished hard dependencies and invalid relationship data, so that ready results are trustworthy.
22. As a contributor, I want blocked work to include the unfinished hard dependencies responsible, so that I can understand what is preventing progress.
23. As an owner, I want each work item to name a human owner, so that accountability is explicit.
24. As a creator, I want creator provenance retained separately from owner and executor, so that creation history is not confused with current responsibility.
25. As an owner, I want to initiate transfer of ownership, so that accountability changes only through an explicit owner action.
26. As an operator, I want mediated writes checked against the current owner's identity, so that ordinary tool usage discourages unauthorized edits.
27. As a user, I want the product to explain that operator checks are local process guardrails rather than authentication, so that no one mistakes them for a security boundary.
28. As a project maintainer, I want executor metadata to remain distinct from ownership and authorization, so that assigning work does not grant write rights.
29. As a project maintainer, I want custom fields to support string, integer, boolean, and homogeneous scalar lists, so that common metadata shapes can be represented without a general schema language.
30. As a project maintainer, I want custom fields optional unless explicitly required, with optional static defaults applied on creation, so that teams can adopt fields gradually.
31. As a contributor, I want unset optional fields omitted rather than serialized as null, so that records remain concise and missing data has one meaning.
32. As a project maintainer, I want undeclared custom fields and invalid values reported as errors, so that the persisted schema remains predictable.
33. As a contributor, I want custom-field filtering by presence, equality, and supported typed comparisons, so that I can find records without writing arbitrary query code.
34. As a contributor, I want list values compared as whole values or tested for explicit element membership, so that list filtering is unambiguous.
35. As a contributor, I want custom-field sorting and projection with deterministic missing-value behavior, so that query output is stable and scriptable.
36. As an agent, I want stable versioned structured results and errors from machine interfaces, so that integrations can handle success and failure without parsing human prose.
37. As a contributor, I want a shared domain library behind CLI and MCP, so that commands and agent tools enforce the same validation, relationship, authorization, and concurrency rules.
38. As an agent, I want MCP to expose read/search and authorized write operations, so that agent workflows can use Waymark through a machine interface.
39. As a maintainer, I want configuration and work-item formats to evolve under a single command-managed compatibility version, so that migrations are explicit and files are never guessed at or silently downgraded.
40. As a maintainer, I want migration dry-runs, complete prevalidation, diagnostics, backups, and rollback/recovery behavior, so that format changes do not silently lose project data.
41. As a contributor, I want deterministic serialization and stable ordering, so that repeated operations do not create meaningless Git diffs.
42. As a contributor, I want authoritative item files to remain correct even when the in-process projection is unavailable, so that a cache cannot become a second source of truth.
43. As a contributor, I want the in-process static projection synchronized from authoritative files when needed, so that indexed relationship queries do not silently return stale data.
44. As a contributor, I want write conflicts detected rather than silently overwritten, so that concurrent edits preserve each contributor's changes.
45. As a contributor, I want batch writes prevalidated and staged with application-level rollback, so that partial results are reported honestly and avoidable partial changes are prevented.
46. As a reviewer, I want Git history and diffs to show changes to work items, so that change inspection uses the repository's existing history.
47. As a maintainer, I want no separate application audit log, so that Waymark does not imply an independently authenticated or tamper-proof audit system.
48. As a user, I want no import or export feature, so that this product scope stays focused on native Git-backed work-item management.
49. As a project member, I want a read-only Web overview of the work tree, so that I can understand project structure and progress visually without risking project-data changes.
50. As a project member, I want combined and category-grouped tree views, with combined view as the default, so that I can browse hierarchy globally or by project grouping.
51. As a project member, I want cross-category children to remain under their actual parent in grouped view, so that categories do not falsify hierarchy.
52. As a project member, I want stable category shape cues and category labels, so that category remains recognizable even when the tree is grouped by another dimension.
53. As a project member, I want colors for open/ready, in-progress, blocked, and closed work, so that lifecycle and derived blockage are legible; color must not reinterpret arbitrary custom status fields.
54. As a project member, I want parent-child edges solid and optional dependency edges dashed, so that hierarchy and blocking remain visually distinct.
55. As a project member, I want cross-category dependency targets represented only while dependency overlays are enabled, so that the grouped view stays uncluttered when dependencies are hidden.
56. As a project member, I want to open a read-only detail view showing the item's body, category, actual status, blocked reason, owner/executor, and relationships, so that I can inspect work without editing it.
57. As a project member, I want to expand or collapse individual tree nodes and all nodes, so that I can control overview density.
58. As a project member, I want view, expansion, dependency-visibility, and scroll state remembered in my browser, so that I can resume browsing without writing preferences into project data.
59. As a mobile user, I want the tree to remain vertical with compact indentation, wrapping, and vertical scrolling, so that it remains usable on narrow screens.
60. As a project member, I want empty projects and invalid parent references explained without hiding valid data, so that the view remains useful even when there is nothing to show or the data needs repair.

## Implementation Decisions

### Domain model and persistence

- A work item is the core tracked unit. Its stable ID is immutable and project-unique, independent of category, title, and storage path.
- Work-item files are the sole authoritative data. Use readable Markdown bodies and YAML frontmatter with strict, deterministic serialization. Unified edit supports full body replacement: absent body leaves it unchanged, empty body clears it, and non-empty body is canonicalized to one trailing newline; CLI supports editor/`--body`/`--body-file`, while MCP accepts structured body input.
- Project configuration lives at the fixed Waymark configuration location; it defines configurable storage root, category set/path/display metadata, operator, custom-field schema, and format version. Do not assume a fixed tickets directory layout.
- Categories are project-defined groupings. They are not hierarchy, lifecycle, or workflow semantics; the initial model has no category-specific custom-field schemas.
- Use one command-managed `config_version` to identify configuration and frontmatter compatibility. It advances for incompatible format/semantic changes, not routine writes. Do not use an independent `frontmatter_version`.
- Current and supported older versions may be read according to explicit compatibility rules; older data may be lazily upgraded on normal write. Reject future-version, invalid, or unparseable data with locatable diagnostics; do not silently default, recreate, discard, repair, or downgrade it.
- Explicit migrations support dry-run, affected-file summaries/warnings, full target-set validation before writes, backups under `.waymark/`, deterministic output, staged replacement, atomic per-file replacement, and rollback/recovery reporting. Backups must not be silently overwritten, and backup failure prevents migration writes. Archive metadata is stored in the original work-item frontmatter; migration backups remain under `.waymark/`.

### Work-item lifecycle and relationships

- Core lifecycle states are `open`, `in_progress`, and `closed`. Closed means completed and no longer blocking dependents. `blocked` is a derived condition, not a writable state.
- A non-closed item is blocked when it has unfinished hard dependencies. A closed item cannot simultaneously be considered blocked.
- Archive is an irreversible-through-Waymark logical deletion distinct from closed. It adds `archived_at` and optional `archive_reason` to the original frontmatter without updating `updated_at`; it preserves the file and ordinary metadata, is owner-only, blocks when unarchived children exist, and provides no Waymark restore. Archived items are excluded from ordinary active views but are readable by ID and through explicit archive-aware queries.
- Archive requires a complete fresh in-process projection and batch-cleans inbound core dependencies, parent, and soft links; it does not inspect body or custom-field references. It updates affected active files' `updated_at`, rejects new relationships to archived items, and reports conflicts/rollback failures structurally.
- Physical deletion is not an ordinary initial operation. If introduced later, it must be explicit and refuse to break references unless a deliberate repair/migration is performed.
- Each item has at most one parent. Parent-child hierarchy is independent of hard dependencies; parent does not block, imply dependency, or cascade lifecycle. Parent can cross categories.
- Hard dependencies are directed and block readiness until the prerequisite is closed. Soft links do not block. Relationship targets remain resolvable after archive. Invalid, cyclic, missing, or otherwise damaged relationships produce actionable diagnostics and must not be silently reinterpreted.

### Accountability and writes

- Owner is required human accountability; creator is provenance; executor is distinct metadata that may be project-defined (for example as a custom field). None of these terms are interchangeable.
- Creation may assign an owner other than the creator. Only the current owner may initiate transfer; the transfer takes effect immediately.
- Every later mediated write checks the configured declared operator against the current owner. This is a workflow guardrail, not authentication: users can directly edit files or alter local configuration. Executor metadata grants no authorization.
- Batch writes prevalidate and stage all targets under a project-level write lock. Use optimistic content-fingerprint conflict detection and reject conflicting changes for reread/retry rather than auto-merge or overwrite.
- Treat batch writes as application-level all-or-nothing with rollback/recovery material, not as a filesystem multi-file atomic transaction. Report clearly if rollback fails. Authoritative file writes and derived-index updates are not one transaction: successful file writes remain successful if index refresh fails, and the index is marked/recoverable for rebuild.

### Custom fields and queries

- Custom fields are project-declared, uniquely named data with no field-name-specific behavior. Initial value types are string, integer, boolean, and homogeneous lists of scalar values; no nested objects/lists or mixed lists. Enum is a string convention, not a runtime type. No special timestamp or reference type in v1.
- Fields are optional by default; `required=true` can require a value. An optional static default may be applied on creation. Omit unset values; `null` is invalid.
- Reject undeclared keys and values with invalid shapes. Custom fields do not add reference resolution, authorization, or workflow semantics.
- Provide a finite generic query vocabulary: exists/missing and equality/inequality for declared fields; ordering comparisons for strings/integers; case-sensitive prefix/substring for strings; whole-list equality/inequality plus explicit element `contains`. Do not support arbitrary expressions, nested paths, or nested query semantics.
- Sort only strings and integers. Missing values sort after set values in both directions; stable ID breaks ties. Booleans/lists are not sortable.
- Projection supports named custom fields and all-set-fields mode; omit unset fields and return structured errors for unknown names. Query operand types must match declared field types; no implicit conversion. Persisted schema violations are locatable data errors, not silently omitted records.

### Interfaces and machine contracts

- Implement domain operations in a shared function library; CLI and MCP are adapters that use the same semantics and validations. Other interfaces, if present, must reuse these semantics.
- Core modules return typed, enumerable errors rather than using human-readable messages as program contracts. Normal domain and operational failures must not use panic as control flow; unexpected panics are program faults handled at the outermost process boundary.
- Define one Waymark error taxonomy in the shared types/core module. Stable string codes distinguish failures such as `ITEM_NOT_FOUND`, `INVALID_INPUT`, `INVALID_DATA`, `DEPENDENCY_CYCLE`, `PARENT_CYCLE`, `OWNER_GUARD`, `WRITE_CONFLICT`, `UNSUPPORTED_VERSION`, `MIGRATION_FAILED`, `INDEX_STALE`, `INDEX_CORRUPT`, `IO_ERROR`, `GIT_ERROR`, and `INTERNAL_ERROR`. The initial list may grow compatibly, but callers must not classify errors from `message` text.
- Each structured error includes the envelope version, stable `code`, human-readable `message`, a small typed `details` payload, optional `location` (work item, file, field, or relationship), and `retryable`. Details carry machine-usable information such as a dependency cycle path, conflict fingerprint, or migration summary; they are schema-controlled rather than an unbounded string bag.
- All machine responses use a versioned envelope: success is `{v, ok: true, data, _meta}` and failure is `{v, ok: false, error}`. Errors are structurally identical across adapters at the domain level, while human output remains adapter-specific.
- CLI maps Waymark error codes to a small stable exit-code set: `0` success, `1` usage error, `2` not found, `3` invalid input/data, `4` conflict or owner/operator guardrail rejection, `5` filesystem/Git/environment failure, `6` unsupported version or migration failure, and `7` internal failure. Do not allocate one numeric exit code per fine-grained error code.
- MCP reuses Waymark's business error codes and structured error details; it does not invent a second MCP-specific business taxonomy or CLI-style exit codes. MCP/JSON-RPC protocol errors remain separate and are used only for malformed protocol requests, unknown tools, or transport/protocol failures.
- CLI provides project initialization/configuration/diagnostics, work-item creation/read/edit/move/archive, lifecycle and relationship operations, list/tree/leaf/search/query/readiness/blocked views with explicit archive selection, validation and migration, Git history/diff access where available, stats, projection synchronization diagnostics, and generated agent documentation. Exact command spelling is an implementation detail unless a command is needed to fulfill a user-visible capability.
- MCP is in scope for agent-facing read/search and mediated writes. MCP writes use the same owner/operator, schema, relationship, batch, and concurrency checks as CLI.
- Wayfinder-specific map, decision-ticket, claim-prompt, and frontier semantics do not belong in Waymark. Generic tree and leaf queries do; a leaf means an item with no child items.
- No import or export capability is included.

### Diagnostics, health checks, and batch failures

- Provide a diagnostic/health-check operation that reports data integrity findings separately from command execution failure. Findings include `severity`, stable `code`, human-readable `message`, optional `location`, and `fixable`.
- Diagnostics cover malformed or oversized frontmatter, duplicate or mismatched stable IDs, invalid fields, missing references, dependency/parent cycles, unsupported versions, and index divergence/corruption when an index exists. A warning-only report succeeds by default; strict mode returns the validation exit class when error-severity findings remain.
- Automatic repair is limited to clearly safe canonicalization and rebuild operations. Never remove a relationship, discard data, or infer user intent automatically. Every repair reports what changed and what remains.
- Relationship-cycle detection returns a structured cycle path or diagnostic; it must not panic. A scan that cannot safely validate the complete graph fails explicitly rather than validating a partial graph as if it were complete.
- Batch writes prevalidate all targets before writing. If validation fails, no target is changed. Execution conflicts, I/O failures, or rollback failures return one batch-level structured error with per-target outcomes such as `validated`, `written`, `conflicted`, `rolled_back`, and `rollback_failed`; partial success is never presented as ordinary success.
- Error details and diagnostics are tested against published schemas or equivalent contract tests so adding a code or field is deliberate and machine clients can depend on the versioned shape.

### Index and concurrency

- Work-item Markdown files are the sole authority. Waymark v1 persists no JSON index; a complete in-process static frontmatter projection is disposable and rebuilt or incrementally synchronized from files.
- Projection entries retain source paths, fingerprints, parse diagnostics, and forward/reverse core relationships. Derived readiness and graph values are computed dynamically and never persisted.
- Local reads may use scoped temporary parses; global queries, archive, and cross-file writes require a complete fresh projection. Long-lived contexts perform lightweight freshness checks, invalidating changed/added/deleted/moved or uncertain entries; incomplete projections cannot be presented as complete results.
- Writes occur under the project lock with fingerprint conflict checks, then synchronize and publish a new projection. A bounded LRU may cache body/details/query data only; it cannot evict relationship nodes or weaken freshness/completeness rules.

### Read-only Web

- Web is strictly read-only, simple, visual, and an overview rather than a search/filter replacement. It offers combined and category-grouped top-to-bottom trees of non-archived work items; default to combined and remember the selected view.
- Do not invent a synthetic root: every parentless item is a root. In grouped view, cross-category children remain under their actual parent. Category identity uses stable node shapes plus category name/abbreviation; shape assignments cycle stably when categories outnumber shapes.
- Closed items remain visible. State colors distinguish open/ready, in-progress, derived blocked, and closed. Blocked is limited to non-closed items with unfinished hard dependencies and also has a concise text marker; detail shows specific unfinished dependencies. Do not derive colors from custom/category-specific status fields.
- Solid edges show parent-child hierarchy. Optional dashed dependency edges have a simple global show/hide control, remembered separately per view. In category-grouped mode, a displayed item depending on an item in another category adds a reference node and dashed edge. Use the referenced item's parent as the cross-category connection point when it has one; otherwise use the referenced item itself. Hiding dependencies also hides those reference nodes.
- Selecting an item name opens read-only detail showing body, category, actual status, blocked reason, owner/executor, and relationships.
- Support per-node expand/collapse and Expand all/Collapse all. Keep expansion state separately for each view.
- Persist selected view, per-view expansion, per-view dependency visibility, and scroll position in the current browser only. Refresh uses the browser; no in-page refresh button. Do not write view preferences or any project data from Web.
- On narrow screens keep the vertical tree with compact indentation, wrapping, and vertical scrolling. Show a concise empty state. Combined view remains usable when no categories are configured. Show valid trees when parent references are invalid/missing, and place unattached items in a clearly explained unresolved-relations area rather than hiding them or treating them as ordinary roots.
- This is the complete Web scope for this specification: no extra Web features or separate archive browser.

### History and explicit scope

- History/diff uses Git history and worktree/baseline comparisons where available. No separate application audit log, authenticated attribution, or team audit service is required.
- Do not commit to delivery phases or a release schedule; the spec defines capabilities and behavior.
- Do not provide import/export.

## Testing Decisions

- Test externally visible behavior and stable contracts, not internal helper structure or implementation details.
- Use the shared domain library as the highest-level primary test seam. Exercise lifecycle/readiness, relationship validation, ownership authorization, custom-field validation/query semantics, migration behavior, deterministic file persistence, conflict handling, batch rollback/recovery, and index freshness/rebuild behavior through public operations and temporary project data.
- Test CLI and MCP as adapters: assert argument/protocol-to-domain behavior, stable structured result/error contracts, and that both enforce the same domain rules. Avoid duplicating all domain cases in each adapter suite.
- Test Web at its public UI/API seam: hierarchy and category rendering, work-state/accessibility markers, dependency overlays and cross-category reference behavior, detail contents, expand/collapse and browser persistence, responsive/empty/unresolved states, and rejection/absence of write behavior.
- Good tests assert persisted files, returned results/errors, and rendered user-visible output. For concurrency and migration, assert no silent overwrite/data loss and explicit recovery diagnostics rather than testing lock implementation details.
- The existing Clove reference implementation provides prior art for frontend component/API/query/history tests, but Waymark implementation-specific test conventions and module seams must be established when its codebase exists. Do not treat Clove's feature set as a required test checklist.

## Out of Scope

- Implementing Wayfinder-specific maps, decision tickets, frontier queries, assignment prompts, or claim conventions in Waymark core.
- A search/filter-centric Web application; Web remains an overview and detail browser.
- Web writes, project-data editing, or an in-page refresh control.
- A separate Web archive browser.
- Import/export.
- A separate application audit log or team audit service.
- Physical deletion as an ordinary initial operation.
- Authentication or security guarantees based on local operator/owner configuration.
- Category-specific schemas, custom-field workflow/reference semantics, arbitrary query DSLs, nested custom data, or a general workflow DSL.
- Delivery phases, release sequencing, or a fixed rollout plan.
- TUI, daemon, external tracker synchronization, notifications, plugins, and other capabilities not required by the map decisions.

## Further Notes

- The existing CLI functional-analysis draft predates the Wayfinder decisions and is not authoritative where it conflicts. In particular, the spec decisions here supersede its separate frontmatter version, optional SQLite index, Wayfinder-specific frontier command, import/export, and proposed delivery phases.
- Use the project glossary terms: work item, category, owner, executor, creator, operator, parent, dependency, archive, closed, soft link, custom field, field type, stable ID.
- Clove is the behavioral reference for typed errors, canonical error-code mapping, versioned envelopes, diagnostic findings, strict health checks, and cycle reporting; Waymark adopts these patterns with its own domain codes and without Clove's daemon/plugin-specific error classes.
- Waymark is not yet implemented. The implementation plan should split this spec into tracer-bullet tickets after review; this document does not choose release phases.
