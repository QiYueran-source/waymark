---
schema: 1
id: waym-WMSPEC01
title: 'Waymark functional specification: settled contract'
status: open
type: feature
priority: 2
created: 2026-11-03T03:25:00.000Z
updated: 2026-11-03T03:25:00.000Z
labels: [ready-for-agent]
deps: []
relates: [waym-VS5Z5543]
duplicates: []
supersedes: []
---
## Problem Statement

Teams need a general-purpose work-item tracker that lives inside their Git repository and stays reviewable as ordinary files. They want project-defined categories and metadata rather than a vendor's fixed workflow, a stable identity for every work item that survives renaming and moving, explicit human accountability, and one set of rules shared by the command line and by agents. Concurrent edits must not silently overwrite each other, retired work must be removable from ordinary views without losing history, and people must be able to see the shape of the work at a glance without any risk of changing it.

## Solution

Waymark is a local-first, Git-friendly tracker for project-defined work items. Each work item is a Markdown document with YAML frontmatter, and those files are the sole authority. Project configuration defines the storage layout, the category set, the operator identity, the custom-field schema, and the project time zone. A shared domain library owns every rule; the CLI and the MCP server are adapters over that library and cannot diverge from it. Readiness and blockage are derived, never stored. Archive is an irreversible-through-Waymark logical deletion that leaves the file and its Git history intact. Format compatibility is versioned and changed only through explicit, previewable migration. A separate, strictly read-only Web view renders the work tree.

This specification records only the decisions that are settled. Areas still under design are listed at the end and are deliberately not specified here.

## User Stories

1. As a project maintainer, I want to initialize a Waymark project with an explicit configuration file, so that the tracker knows where and how to store work items.
2. As a project maintainer, I want to configure the storage root and each category's path, so that Waymark fits the repository's existing organization instead of imposing one.
3. As a project maintainer, I want to define project-specific categories with stable keys and separate display names, so that work can be grouped without adopting Waymark-specific workflow categories.
4. As a project maintainer, I want to declare project-wide custom fields with basic scalar types, so that work items can carry metadata my team cares about.
5. As a project maintainer, I want to declare a project time zone, so that timestamps are shown and local time input is interpreted in my team's zone.
6. As a contributor, I want every work item to have a stable identity independent of its title, category, and path, so that references survive renaming and moving.
7. As a contributor, I want each work item stored as readable Markdown with typed YAML frontmatter, so that Git can review, diff, and merge project data.
8. As a contributor, I want files written deterministically, so that repeated operations do not produce meaningless diffs.
9. As a contributor, I want unset optional values omitted rather than serialized as null, so that missing data has exactly one representation.
10. As a contributor, I want to create a work item with a title, category, and human owner, so that accountability is explicit from the first moment.
11. As a contributor, I want to create a work item together with its initial parent, dependencies, and soft links, so that a new item can arrive already placed in the graph.
12. As a contributor, I want an invalid initial relationship to fail the whole creation, so that no half-formed item is written.
13. As a contributor, I want to read one work item by stable ID and see its fields, body, relationships, and derived readiness, so that I understand its current context.
14. As a contributor, I want to replace a work item's title, so that it stays a short, single-line, non-empty label.
15. As a contributor, I want to replace a work item's body wholesale, or clear it, so that the Markdown document reflects exactly what I intend.
16. As a contributor, I want a non-empty body to end in exactly one newline without my leading or internal whitespace being touched, so that formatting stays stable across edits.
17. As a contributor, I want to set and clear individual custom fields in one operation, so that related metadata changes land together.
18. As a contributor, I want an update that changes nothing to write nothing and leave the modification time alone, so that no-op edits do not pollute history.
19. As a contributor, I want title, body, and custom fields to be the only things ordinary editing can change, so that lifecycle, ownership, and relationships cannot be altered by accident.
20. As a contributor, I want to set a work item's status to open, in progress, or closed, so that its lifecycle is explicit.
21. As a contributor, I want closed work to stop blocking its dependents, so that downstream work becomes actionable.
22. As a contributor, I want the close timestamp set and cleared automatically as status crosses closed, so that I never maintain it by hand.
23. As a contributor, I want blocked work to list the specific unfinished dependencies responsible, so that I can see what is in the way.
24. As a contributor, I want a missing dependency target to both block readiness and be reported as dangling, so that a broken reference cannot look like readiness.
25. As a contributor, I want blocked to be a derived condition rather than a status, so that status and blockage can never contradict each other.
26. As a contributor, I want one parent per work item that may cross categories, so that containment does not require a category-shaped hierarchy.
27. As a contributor, I want parenthood to neither block nor cascade lifecycle, so that structure and execution order stay independent.
28. As a contributor, I want parent cycles rejected when I create them, so that the hierarchy stays a hierarchy.
29. As a contributor, I want directed hard dependencies separate from parenthood, so that only real prerequisites affect readiness.
30. As a contributor, I want dependency cycles rejected when I create them, so that readiness remains computable.
31. As a contributor, I want symmetric, non-blocking soft links, so that related work can be associated without changing readiness.
32. As a contributor, I want a soft link stored consistently on both sides, so that the association never appears one-sided.
33. As an owner, I want to transfer ownership of a work item I own, so that accountability moves deliberately rather than by silent edit.
34. As an owner, I want transfer to take effect immediately, so that the previous owner loses write authority at once.
35. As an operator, I want every mediated write checked against the item's current owner, so that ordinary tool use discourages accidental edits to other people's work.
36. As a user, I want the product to state plainly that this check is a workflow guardrail rather than authentication, so that nobody mistakes it for a security boundary.
37. As a project maintainer, I want executor to be project metadata rather than a core field, so that assigning work never grants authority.
38. As a creator, I want my creation recorded as immutable provenance separate from ownership, so that history and responsibility stay distinct.
39. As an owner, I want to archive a work item as an irreversible-through-Waymark logical deletion, so that retired work leaves ordinary views while its file and Git history remain.
40. As an owner, I want archiving refused while the item still has unarchived children, so that a subtree cannot be silently detached.
41. As a contributor, I want archiving to strip inbound structured relationships from active files, so that active graph results stay coherent.
42. As a contributor, I want archiving to leave body mentions and custom-field values untouched, so that Waymark never guesses at free-form intent.
43. As a contributor, I want archived items excluded from ordinary collections, so that active views stay focused.
44. As a contributor, I want an explicit archive-range selector on collections, so that I can look at retired work deliberately.
45. As a contributor, I want to read a single archived work item by ID, so that I can inspect retired work without mixing it into active results.
46. As an owner, I want a dry-run for archiving, so that I can see every planned relationship removal before committing.
47. As a contributor, I want archived items to reject every later write, so that archive is final within the tool.
48. As a contributor, I want no Waymark restore command, so that returning retired work is a deliberate Git operation rather than a hidden state change.
49. As a project maintainer, I want to add a custom field without rewriting existing items, so that schema growth is cheap.
50. As a project maintainer, I want removing a custom field to delete its values everywhere in one atomic operation, so that the library is never left half-invalid.
51. As a project maintainer, I want field type changes, renames, and optional-to-required changes rejected by migration, so that no converter silently guesses at my data.
52. As a contributor, I want to filter work items by declared custom fields with equality, comparison, and string matching, so that I can find work without writing query code.
53. As a contributor, I want list-valued fields compared as whole values or tested for explicit membership, so that list filtering is unambiguous.
54. As a contributor, I want deterministic sorting with a stable-ID tie-breaker, so that query output is scriptable.
55. As a contributor, I want to project only the fields I asked for, so that large responses stay small.
56. As a contributor, I want query results paginated against a single snapshot, so that paging cannot skip or repeat items mid-scan.
57. As an agent, I want every machine response in a versioned envelope, so that integrations can rely on a stable shape.
58. As an agent, I want stable, enumerable error codes with typed details and locations, so that failures can be handled without parsing prose.
59. As an agent, I want MCP to expose the same capabilities and the same rules as the CLI, so that switching adapters never changes behaviour.
60. As a contributor, I want the CLI to map error classes onto a small, stable set of exit codes, so that shell scripts can branch reliably.
61. As a contributor, I want the in-process projection rebuilt from files whenever it is stale, so that no cache can become a second source of truth.
62. As a contributor, I want global graph queries refused when the projection is incomplete, so that a partial graph is never presented as complete.
63. As a contributor, I want machine results to state their source, completeness, and freshness, so that I can tell a local read from a graph answer.
64. As a contributor, I want write conflicts detected by fingerprint rather than overwritten, so that concurrent edits preserve both people's work.
65. As a contributor, I want multi-document operations validated before anything is written, so that a failure changes nothing.
66. As a contributor, I want an honest report when rollback fails, so that I never believe a half-applied operation succeeded.
67. As a maintainer, I want configuration and work-item compatibility governed by one declared version, so that evolution is explicit.
68. As a maintainer, I want the configuration revision to advance only when configuration content actually changes, so that the counter means something.
69. As a maintainer, I want no configuration edit to change the on-disk format generation, so that housekeeping never forces a library-wide rewrite.
70. As a maintainer, I want a work item's format generation stored in the file, so that files from a different generation are detectable.
71. As a maintainer, I want stale-generation items to become read-only rather than silently upgraded, so that format changes are deliberate.
72. As a maintainer, I want stale-generation items still listed and shown, so that pending migration never hides work.
73. As a maintainer, I want a reader for the current and the immediately previous generation, so that the read-only window is real without unbounded legacy support.
74. As a maintainer, I want to preview a migration before running it, so that I can see every affected file.
75. As a maintainer, I want a migration to re-derive and re-check at execution time, so that a stale preview cannot be applied.
76. As a maintainer, I want storage relocation to refuse existing, overlapping, case-colliding, or symlinked targets, so that no user data is ever overwritten.
77. As a maintainer, I want a category-key migration to rewrite the persisted category value everywhere, so that keys stay consistent with paths.
78. As a maintainer, I want migration to work outside a Git repository but to warn about the loss of Git-based recovery, so that the tool never pretends a safety net exists.
79. As a contributor, I want diagnostics reported separately from command failure, so that a healthy run with findings is still a healthy run.
80. As a contributor, I want diagnostics to be report-only for business data, so that the tool never deletes a relationship or infers my intent.
81. As a project member, I want a read-only Web view of the work tree, so that I can see structure and progress without any risk of modifying data.
82. As a project member, I want a combined tree by default and a category-grouped alternative, so that I can read the hierarchy globally or by grouping.
83. As a project member, I want cross-category children to stay under their real parent in the grouped view, so that grouping never falsifies the hierarchy.
84. As a project member, I want category shape cues and work-state colours, so that grouping and lifecycle remain legible at a glance.
85. As a project member, I want solid parent edges and optional dashed dependency edges, so that containment and blocking stay visually distinct.
86. As a project member, I want a read-only detail view with body, status, blocked reason, ownership, and relationships, so that I can inspect work without editing it.
87. As a project member, I want expand/collapse per node and for the whole tree, so that I can control density.
88. As a project member, I want view, expansion, dependency visibility, and scroll position remembered in my browser, so that browsing resumes where I left off without writing into project data.
89. As a mobile user, I want the tree to stay vertical with compact indentation and vertical scrolling, so that it remains usable on a narrow screen.
90. As a project member, I want an empty project and broken parent references explained rather than hidden, so that the view stays truthful when data needs repair.
91. As a reviewer, I want changes to work items visible in Git history and diffs, so that review uses the repository's existing mechanism.
92. As a maintainer, I want no separate application audit log, so that Waymark never implies tamper-proof attribution it cannot provide.

## Implementation Decisions

### Domain model and persistence

- A work item is the core tracked unit. Its stable ID is immutable, project-unique, and independent of category, title, and storage path. Generate it as `wm-` plus 12 Crockford Base32 characters, persist it lowercase, and accept it case-insensitively; detect collisions within the project and regenerate before committing. Ordinary creation does not accept a caller-supplied ID.
- Work-item files are the sole authoritative data. They are Markdown documents with YAML frontmatter, serialized strictly and deterministically.
- The core frontmatter fields are `id`, `title`, `category`, `status`, `owner`, `creator`, `created_at`, `updated_at`, `closed_at`, `parent`, `dependencies`, `soft_links`, `custom_fields`, and the format generation marker. The Markdown body is document content after the frontmatter, not a frontmatter field. `id`, `title`, `category`, `status`, `owner`, `creator`, `created_at`, and `updated_at` are required; `body` may be empty; `closed_at`, `parent`, relationship lists, and `custom_fields` may be absent. Absent relationship lists mean empty lists; absent custom fields mean no custom values. Never serialize an unset optional field as `null`.
- `title` must be non-empty after trimming surrounding whitespace and must be single-line; internal whitespace is preserved.
- `owner` and `creator` are non-empty declared identity strings. Trim surrounding whitespace, preserve the rest, and compare exact normalized strings without case folding and without resolving them to system accounts. The operator/owner guard uses the same rule.
- `created_at` and `updated_at` are UTC RFC 3339 timestamps generated by Waymark. Both are set on creation. Every successful ordinary content or metadata change updates `updated_at`; archive is the explicit exception and does not. `closed_at` is set when status becomes `closed`, cleared when it leaves `closed`, and describes only the most recent close transition; it is not an audit record.
- Files are named from the persisted lowercase stable ID plus a fixed Markdown extension. Titles never enter filenames.
- Project configuration lives at a fixed location and defines: the storage root, the category set with keys, paths, and display names, the operator identity, the custom-field schema, the project time zone, the format generation, and the configuration revision. No fixed tickets directory layout is assumed.
- Categories are project-defined groupings. A category key is project-unique lowercase ASCII, starting with a letter and continuing with lowercase letters, digits, or hyphens; it is separate from the display name. Work items persist the key. Display-name changes never move files. Key and path changes are explicit migrations. Removing a category that still has items is rejected until those items are reassigned. Categories carry no hierarchy, lifecycle, or workflow semantics, and there are no category-specific field schemas.

### Lifecycle, relationships, and archive

- Status is exactly `open`, `in_progress`, or `closed`. Any explicit transition among them is allowed for an active item. Repeating the current status changes nothing. Status changes update `updated_at`.
- `blocked` is a derived condition, never a writable status and never persisted. A non-closed item is blocked when it has an unfinished or missing hard dependency. A closed item is never blocked. `blocked_by` is the computed list of unfinished hard dependencies and is never persisted.
- Each item has at most one parent. Parentage is independent of hard dependencies: it does not block, does not imply a dependency, and does not cascade lifecycle. Parents may cross categories. Parent cycles are rejected at write time.
- Hard dependencies are directed and block readiness until the prerequisite is closed. Dependencies and soft links are persisted as unique, deterministically sorted lists of stable IDs; duplicates and self-references are rejected. Dependency cycles are rejected at write time. Missing dependency targets block readiness and are reported as dangling.
- Soft links are symmetric and non-blocking. Adding one makes it visible from both items, and storage must never leave a one-sided link.
- Relationship mutations are dedicated validated operations, never generic field patches. Multi-target requests prevalidate and commit atomically; a single invalid target fails the whole request.
- Archive is an irreversible-through-Waymark logical deletion, distinct from `closed`. It is available from any status and changes neither `status` nor `closed_at`.
- Archiving preserves the file and all existing metadata, including body, custom fields, and relationship fields. It adds `archived_at` and an optional `archive_reason`, records no `archived_by`, and does not update `updated_at`.
- Only the current owner may archive, subject to the operator guardrail. Re-archiving returns `ALREADY_ARCHIVED` without altering the recorded archive metadata.
- An item with any unarchived child is rejected with `HAS_ACTIVE_CHILDREN`; archiving never cascades.
- Archived items reject every later mediated write: title, body, custom fields, status, owner transfer, relationships, and re-archive.
- Archive removes inbound structured relationships pointing at the item from active files — `dependencies`, `parent`, and `soft_links` — and updates those files' `updated_at`. It never inspects or edits body mentions or custom-field values.
- Archive requires a complete, fresh projection, performs full prevalidation under the project lock, writes the target and affected files as one application-level operation, and publishes a new projection only on success. Any conflict, incomplete projection, parse error, or write failure prevents the operation; a rollback failure is reported explicitly.
- Archive supports a dry run that runs the same completeness, ownership, child, relationship, and conflict checks and reports the target and every planned removal without writing.
- Archived items are excluded from ordinary collections, trees, readiness, and statistics. An explicit archive-range selector chooses archived-only; there is no implicit active-plus-archived collection. Reading one item by ID is the deliberate exception and reports archive metadata.
- An archived item's own relationship fields remain as historical context and do not participate in active graph calculations. New relationships cannot target an archived item. Waymark provides no restore or unarchive command; restoring means removing the archive marker in a historical Git version.
- Physical deletion is not an ordinary operation.

### Accountability and writes

- Owner is required human accountability; creator is immutable provenance; executor is project-defined metadata (for example a custom field) and is not a core field. None of the three is interchangeable.
- Creation may assign an owner other than the creator.
- Only the current owner may initiate an owner transfer. It takes effect immediately, updates `updated_at`, rejects empty or whitespace-only identities, and is forbidden for archived items. The previous owner loses mediated-write authority at once.
- Every mediated write compares the declared operator with the item's current owner. This is a workflow guardrail, not authentication: users can edit files or the configuration directly. Executor metadata grants no authority.
- Writes are staged under a project-level lock with optimistic content-fingerprint conflict detection. A conflicting change is rejected for reread and retry; there is no automatic merge and no silent overwrite.
- An operation that inherently writes several documents — archive relationship cleanup, migration, multi-target relationship writes, and configuration or schema changes — is application-level all-or-nothing with rollback and recovery material, not a filesystem multi-file transaction. Rollback failure is reported explicitly. Projection refresh is derived work: after a successful write, a refresh failure leaves the documents committed, marks the projection incomplete and recoverable, and reports that synchronization needs recovery rather than reporting partial success.
- Waymark exposes no cross-work-item batch command. Each write targets exactly one work item; there is no multi-ID form, no `items` array, and no mixed-operation array.

### Custom fields and queries

- Custom fields are project-declared, uniquely named data with no field-specific behaviour. The types are string, integer, boolean, and homogeneous lists of scalars. Nested objects, nested lists, and mixed lists are invalid. Enum is a string convention, not a runtime type. There is no reference or timestamp type in this version.
- Field keys are lowercase ASCII `snake_case`. Core and computed names are reserved.
- Fields are optional by default; a field may be declared required. An optional static default may be applied at creation. Unset values are omitted; `null` is invalid.
- Undeclared keys and invalid value shapes are rejected as locatable data errors, never silently dropped.
- Custom values carry no reference resolution, authorization, or workflow semantics. A stable ID inside a custom value is not a relationship.
- The query vocabulary is finite: exists/missing, equality/inequality, ordering comparison for strings and integers, case-sensitive prefix and substring for strings, whole-list equality/inequality, and explicit element `contains`. No arbitrary expressions, nested paths, or nested query semantics.
- Only strings and integers sort. Booleans and lists do not. Missing values sort after present values in both directions, and the stable ID breaks ties.
- Projection supports a named set of fields or all set fields. Unknown names return a structured error and unset fields are omitted. Query operand types must match the declared field type; there is no implicit conversion.
- Collection queries are active-only by default. Structured list/query reads frontmatter and custom fields and never the body. Search may read title, frontmatter, and body and follows the same archive-range rules.
- Results use deterministic ordering, stable-ID tie-breakers, and pagination against a single snapshot.

### Editing

- Ordinary editing is split by data area into three domain operations: set title, replace body, and update custom fields. There is no generic frontmatter patch.
- Title is full replacement: trim surrounding whitespace, require non-empty and single-line, and support no patch, append, or regex form.
- Body is full replacement. An explicit empty value clears it. Non-empty body is canonicalized to exactly one trailing newline without trimming leading or internal whitespace. Body content is never interpreted as a relationship language.
- Custom-field updates take an explicit change map. Each entry is exactly `set` with a value or `clear`; both together, `null`, unknown keys, and clearing a required field are rejected. The whole change set is validated before a single write.
- Each operation changes only its own data area plus `updated_at` when the effective value changes. An unchanged request performs no write and no timestamp update.
- Every edit requires the operator to match the current owner, rejects archived items, and uses fingerprint conflict detection with one atomic document replacement per target.
- The three edit operations return the complete resulting work item plus a `changed` flag; custom-field updates also return the changed field names.
- There is no interactive editor in this version. The CLI accepts direct body text or a body file; MCP accepts structured values and never launches an editor.
- CLI custom-field values are resolved against the declared schema: integers use decimal integer syntax, booleans accept only `true` or `false`, lists require a JSON array, and strings accept an unambiguous token or a JSON string literal.

### Interfaces and machine contracts

- Every domain operation lives in a shared function library. The CLI and the MCP server are adapters that share its semantics and validations. Any future interface reuses it.
- CLI and MCP have equal capability coverage. Neither is a reduced surface, and safety behaviour such as dry runs, complete prevalidation, and structured errors is shared rather than adapter-specific.
- Core modules return typed, enumerable errors instead of using human-readable text as a contract. Normal failures are not signalled by panics; unexpected panics are program faults handled at the process boundary.
- One Waymark error taxonomy is defined in the shared core. Codes are stable strings; the current set includes item-not-found, invalid input, invalid data, dependency cycle, parent cycle, owner guard, write conflict, already archived, has active children, incomplete projection, unsupported version, and migration failures including path conflict, unsupported generation, and precondition failure. Callers must never classify errors from message text.
- Each structured error carries the envelope version, a stable code, a human-readable message, a small typed details payload, an optional location identifying the work item, file, field, or relationship, and a retryability flag.
- Machine responses use a versioned envelope: success is `{v, ok: true, data, _meta}`; failure is `{v, ok: false, error}`. Errors are structurally identical across adapters; human output stays adapter-specific.
- The CLI maps error classes onto a small stable exit-code set: success, usage error, not found, invalid input or data, conflict or guardrail rejection, filesystem/Git/environment failure, unsupported version or migration failure, and internal failure. Fine-grained error codes do not each get a number.
- MCP reuses the business error codes and structured details and does not invent a second taxonomy. JSON-RPC protocol errors stay separate and cover only malformed requests, unknown tools, and transport failures.
- Capability coverage includes project initialization, configuration reading, validation and diagnostics, work-item creation, reading, editing, moving and archiving, lifecycle and relationship operations, collection and tree queries with explicit archive ranges, custom-field queries, readiness and blocked views, statistics, migration, projection synchronization, Git history and diff access where available, and generated agent documentation.
- Wayfinder-specific map, decision-ticket, claim-prompt, and frontier semantics do not belong in Waymark. Generic tree and leaf queries do; a leaf is an item with no children.
- There is no import or export capability.

### Versioning and migration

- Configuration carries a version written `major.minor`, meaning format generation and configuration revision.
- The format generation is declared by the installed release. It changes only when Waymark's own file format changes: the core field set, serialization rules, or body rules. No configuration edit can raise it.
- The configuration revision increments by one whenever a Waymark-mediated configuration write actually changes configuration content, and does not increment for a no-op. Hand edits do not move it.
- The revision is informational. Freshness, projection invalidation, conflict detection, and cache invalidation are determined by file fingerprints, never by the revision counter.
- Work-item frontmatter persists only the format generation. A configuration change can therefore never make an existing work item stale.
- Migration has three kinds: version, which rewrites files to a new format generation; schema, which changes custom-field declarations and values; and storage, which relocates the storage root, a category path, or a category key.
- Every migration kind supports a dry run that performs the complete read-only check and reports affected files, and a real run that repeats the same checks before writing. There is no persisted plan artifact, plan identifier, or plan/apply handshake.
- Durable migration backups are not provided; recovery after a successful but wrong migration relies on Git or user copies. A single run is still application-level all-or-nothing with in-run rollback from staged copies, and a rollback failure is reported explicitly.
- Migration is permitted outside a Git repository with a warning that Git-based recovery is unavailable.
- Schema migration supports only adding a custom field, which rewrites no files, and removing one, which deletes the key from every work item including archived ones. Changing a type, renaming a field, changing a list element type, and making an optional field required are rejected and must be handled as an explicit schema change plus a manual or scripted data edit.
- Removing a field declaration is a migration, not a configuration operation: there is no separate configuration command that deletes a field. Its dry run reports the affected file list and count. The real run removes the declaration, increments the configuration revision once, and deletes the key from every work item as one atomic operation; a failure leaves configuration and files unchanged.
- Storage migration takes one mutually exclusive action per run: change the storage root and move every category directory; change one category path and move that directory; or change one category key, which also rewrites the persisted `category` value in every affected work item. Actions are not combined in one run.
- Storage migration prevalidation rejects, before any write: absolute paths and paths escaping the repository root; target paths equal to, inside, or containing an existing category path; an existing non-empty target; a target case-equivalent to an existing path; symbolic links in the target or any component it traverses; source files that are missing, unreadable, or disagree with the projection fingerprint; and an incomplete or stale projection. Any failure aborts the whole migration and reports every diagnostic.
- A work item whose format generation is older than the configured generation is read-only for mediated writes until an explicit version migration runs. Reads, including reading by ID, remain allowed, and collections still include such items, marked as needing migration rather than hidden.
- Waymark retains a reader for the current and the immediately previous format generation; older generations are rejected as unsupported.
- There is no separate compatibility-review command: validation and diagnostics perform the review against the current configuration, and a migration dry run previews the change.
- Ordinary writes never rewrite an older-generation file. Only an explicit migration changes a file's generation.
- Because removing a declared field leaves existing files schema-invalid, that drift is detected by validation as an undeclared key rather than by any version comparison.

### Diagnostics and health checks

- A diagnostic operation reports data-integrity findings separately from command execution failure. Findings carry a severity, a stable code, a message, an optional location, and a fixable flag.
- Diagnostics cover malformed or oversized frontmatter, duplicate or mismatched stable IDs, invalid fields, missing references, dependency and parent cycles, unsupported versions, stale or invalid projections, and source inventory and fingerprint changes.
- A warning-only report succeeds by default; a strict mode returns the validation failure class when error-severity findings remain.
- Automatic repair is limited to clearly safe canonicalization and rebuild operations. Waymark never removes a relationship, discards data, or infers intent, and every repair reports what changed and what remains.
- Cycle detection returns a structured cycle path or diagnostic and never panics. A scan that cannot validate the complete graph fails explicitly rather than validating a partial graph as complete.
- A finding's details are schema-controlled rather than an unbounded string bag, so adding a code or field is a deliberate, tested change.

### Projection, cache, and concurrency

- Work-item files are the sole authority. No JSON index or database is persisted.
- A process may hold a complete in-process static frontmatter projection: parsed core and custom fields, source paths, fingerprints, parse diagnostics, and forward and reverse core relationship helpers. It is disposable and rebuilt from files.
- Derived values — readiness, blockage, blocked-by, children, dependents, and relationship diagnostics — are computed from the current projection and never persisted.
- A projection is either complete and fresh or explicitly incomplete or uncertain. Local reads may use a scoped temporary read and must identify themselves as such. Global collection, tree, readiness, blocked, and statistics queries, archive, migration, and cross-file relationship writes require a complete, fresh projection.
- Long-lived processes may retain a projection and perform lightweight inventory checks before operating. New, deleted, moved, or uncertain files invalidate the relevant entry or the whole projection; stale data is never returned as current. A parse error permits valid local reads but prevents operations that need a complete graph.
- Writes go to documents first under the project lock with fingerprint conflict checks, then synchronize or rebuild the projection. Incremental synchronization is allowed only when the affected scope is known; otherwise the whole projection is rebuilt. Partial projection state is never published as complete.
- A bounded, configurable LRU may cache body, detail, or query-result data for performance only. Cache entries are fingerprint-bound, and no cache setting may weaken freshness, completeness, ownership checks, or archive safety.

### Read-only Web

- Web is strictly read-only, simple, and an overview rather than a search or filter replacement. It renders non-archived work items as top-to-bottom trees: combined by default, or grouped by category, with the selection remembered.
- No synthetic root is invented: every parentless item is a root. In the grouped view, cross-category children remain under their actual parent.
- Category identity uses stable node shapes plus the category name or abbreviation, cycling stably when categories outnumber shapes.
- Closed items remain visible. State colours distinguish open/ready, in-progress, derived blocked, and closed, with a concise text marker for blocked. Colours are never derived from custom or category-specific values.
- Solid edges show parenthood. Optional dashed dependency edges have a simple global show/hide control remembered separately per view. In the grouped view, a displayed item depending on another category adds a reference node and a dashed edge, anchored to the referenced item's parent when it has one; hiding dependencies also hides those reference nodes.
- Selecting an item opens a read-only detail view with body, category, actual status, blocked reason, owner or executor, and relationships.
- Per-node and whole-tree expand/collapse are supported, with expansion state kept separately for each view.
- Selected view, per-view expansion, per-view dependency visibility, and scroll position persist in the current browser only. There is no in-page refresh control, and Web never writes project data or preferences into the repository.
- Narrow screens keep the vertical tree with compact indentation, wrapping, and vertical scrolling. An empty project shows a concise empty state, and the combined view remains usable with no categories configured.
- Invalid or missing parent references do not suppress valid trees; unattached items appear in a clearly explained unresolved-relations area rather than being hidden or treated as ordinary roots.

### History and scope discipline

- History and diff use Git history and worktree or baseline comparisons where available. No separate application audit log, authenticated attribution, or team audit service is implied.
- The specification defines capabilities and behaviour, not delivery phases or a release schedule.

## Testing Decisions

- Test externally visible behaviour and stable contracts, not internal helper structure.
- Use the shared domain library as the highest-level primary seam. Exercise lifecycle and readiness, relationship validation, ownership authorization, custom-field validation and query semantics, archival effects, version and migration behaviour, deterministic persistence, conflict handling, and projection rebuilds through its public operations against temporary project data. A single seam here covers every adapter, because adapters add no business rules.
- Test the CLI and MCP adapters at their own seam for argument and protocol translation, envelope and error-code contracts, and exit-code mapping. Do not duplicate the full domain suite in each adapter; assert that both route to the same behaviour.
- Test the read-only Web at its public UI and API seam: hierarchy and category rendering, state and accessibility markers, dependency overlays and cross-category reference behaviour, detail contents, expand/collapse and browser persistence, responsive, empty, and unresolved states, and the absence of any write path.
- Good tests assert persisted files, returned results and errors, and rendered user-visible output. For concurrency and migration, assert that nothing is silently overwritten and that recovery diagnostics are explicit, rather than asserting lock or staging internals.
- Clove provides prior art for typed error contracts, diagnostics, and adapter tests; Waymark's own module seams will be established when its codebase exists.

## Out of Scope

- Implementing Wayfinder-specific map, decision-ticket, frontier, assignment, or claim semantics in the core.
- Web writes, in-page refresh, and any separate Web archive browser.
- Import and export.
- A separate application audit log or team audit service.
- Physical deletion as an ordinary operation.
- Authentication or security guarantees based on the local operator/owner configuration.
- Category-specific schemas, reference semantics for custom fields, arbitrary query DSLs, nested custom data, and general workflow DSLs.
- Cross-work-item batch commands.
- Delivery phases, release sequencing, and rollout plans.
- TUI, daemon, external tracker synchronization, notifications, and plugins.
- Anything listed under Not Yet Specified below.

## Not Yet Specified

These areas are still under design. They are excluded deliberately: this specification does not decide them, and no implementation should infer an answer.

- The query and reporting surface: which parameters collection, tree, readiness, blocked, search, and statistics operations expose, the pagination model, default ordering, projection parameters, archive-range naming, tree grouping and depth, and what statistics actually count.
- Creation defaults: whether the owner may be omitted with an operator default, the creator rule when creating on someone else's behalf, required-field behaviour at creation, and category or field defaults.
- The configuration command surface and its boundary with migration: which settings are editable through configuration commands versus `migrate storage`, how a category is removed or its items reassigned, and how configuration writes handle atomicity and revision increments.
- Concurrency and lock lifecycle: acquisition, timeout, and recovery from a lock left behind by a crashed process, and contention between a long-lived MCP projection and a short-lived CLI invocation.
- MCP transport and presentation: stdio versus HTTP, tool-name convention, whether resources or subscriptions are exposed, and how business errors sit alongside JSON-RPC protocol errors.
- The frontmatter field name and value shape that carry the format generation.
- Whether projection synchronization remains a public command, what it reports, and whether read paths may trigger it implicitly.
- The final enumerable error-code set and its exit-code mapping.
- Human-readable output shape: command tables, detail views, tree rendering, and whether the CLI shares work-state colours and markers with the Web view.

## Further Notes

- This specification is derived from the design map `Waymark: General-purpose tracker design map` and the working document `Waymark CLI 与 MCP 核心能力设计说明`. Where this document is silent, the map is the record of what is settled and what is fog.
- The previous functional specification and its implementation tickets are retired and must not be implemented from; they are archived under `.inbox/`.
- Use the project glossary terms: work item, category, owner, executor, creator, operator, parent, dependency, archive, closed, blocked, blocked by, ready, soft link, custom field, field type, stable ID.
- Clove is a behavioural reference for typed errors, canonical error-code mapping, versioned envelopes, diagnostics, cycle reporting, and the frontmatter-plus-body document model — not a feature checklist.
- Priority, due date, and assignee are not core fields; use project custom fields for project-specific metadata.
- Waymark is not yet implemented. This document does not choose a delivery plan.
