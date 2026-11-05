---
schema: 1
id: wm-srxj8ztv1zfz
title: Waymark functional specification
category: spec
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:21Z
updated_at: 2026-11-04T00:14:49Z
closed_at: 2026-11-04T00:14:49Z
---
## Problem Statement

Teams need a general-purpose work-item tracker that lives inside their Git repository and stays reviewable as ordinary files. They want project-defined categories and metadata rather than a vendor's fixed workflow, a stable identity for every work item that survives renaming and moving, explicit human accountability, and one set of rules shared by the command line and by agents. Concurrent edits must not silently overwrite each other, retired work must be removable from ordinary views without losing history, and people must be able to see the shape of the work at a glance without any risk of changing it.

## Solution

Waymark is a local-first, Git-friendly tracker for project-defined work items. Each work item is a Markdown document with YAML frontmatter, and those files are the sole authority. Project configuration defines the storage layout, the category set, and the custom-field schema; a separate local user configuration defines the acting operator identity, the display time zone, and the cache settings. A shared domain library owns every rule; the CLI and the MCP server are adapters over that library and cannot diverge from it. One shared registry classifies every business error and diagnostic. Readiness and blockage are derived, never stored. Archive is an irreversible-through-Waymark logical deletion that leaves the file and its Git history intact. Format compatibility is versioned and changed only through explicit, previewable migration. A separate, strictly read-only Web view renders the work tree. Waymark runs on Linux, macOS and Windows; one set of rules applies on every supported platform, and platform-specific statements appear only where behaviour genuinely differs.

This specification records only the decisions that are settled. Platform behaviour is stated as a requirement, not as verified support: native evidence is recorded separately, and no platform is claimed supported before that evidence exists.

## User Stories

1. As a project maintainer, I want to initialize a Waymark project with an explicit configuration file, so that the tracker knows where and how to store work items.
2. As a project maintainer, I want to configure the storage root and each category's path, so that Waymark fits the repository's existing organization instead of imposing one.
3. As a project maintainer, I want to define project-specific categories with stable keys and separate display names, so that work can be grouped without adopting Waymark-specific workflow categories.
4. As a project maintainer, I want to declare project-wide custom fields with basic scalar types, so that work items can carry metadata my team cares about.
5. As a collaborator, I want to declare my own display time zone, so that timestamps are shown and offset-less local time input is interpreted in my local context without changing what my collaborators see.
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
86. As a project member, I want a read-only detail view with body, status, blocked reason, owner, existing custom fields, and relationships, so that I can inspect work without editing it.
87. As a project member, I want expand/collapse per node and for the whole tree, so that I can control density.
88. As a project member, I want view, expansion, dependency visibility, and scroll position remembered in my browser, so that browsing resumes where I left off without writing into project data.
89. As a mobile user, I want the tree to stay vertical with compact indentation and vertical scrolling, so that it remains usable on a narrow screen.
90. As a project member, I want an empty project and broken parent references explained rather than hidden, so that the view stays truthful when data needs repair.
91. As a reviewer, I want changes to work items visible in Git history and diffs, so that review uses the repository's existing mechanism.
92. As a maintainer, I want no separate application audit log, so that Waymark never implies tamper-proof attribution it cannot provide.
93. As a contributor, I want every core capability available through both the CLI and MCP, so that I can choose an interface without losing features.
94. As an agent, I want a stable CLI command tree with explicit arguments and results, so that automation can invoke operations without guessing.
95. As an agent, I want MCP tools that mirror the shared operations, so that tool discovery maps one-to-one onto capabilities.
96. As a project maintainer, I want to read the effective configuration, so that I can confirm the storage layout, category set, and declared fields before writing.
97. As a project maintainer, I want configuration writes to use structured, validated operations, so that no raw patch can corrupt project configuration.
98. As a project maintainer, I want to add a category with a key, path, and display name, so that new grouping becomes available without hand-editing files.
99. As a project maintainer, I want display-name changes to leave work-item files untouched, so that presentation changes cost nothing.
100. As a project maintainer, I want category removal refused while any item still uses the category, so that work cannot be orphaned.
101. As a project maintainer, I want category path and key changes to run only through explicit migration, so that relocation is deliberate and reviewable.
102. As a maintainer, I want to edit my operator, display time zone, and cache settings directly, so that routine housekeeping never touches shared configuration or work-item data.
103. As a maintainer, I want to add an optional custom field as a configuration operation, so that schema growth rewrites nothing.
104. As a maintainer, I want adding a required custom field to be a migration with a default, so that no existing item is left invalid.
105. As a maintainer, I want removing a custom field to be a migration, so that its data deletion is explicit and previewable.
106. As a maintainer, I want a version migration to accept only a target generation the installed release can write, so that I cannot declare an unwritable format.
107. As a maintainer, I want a migration dry run to report every affected file and count, so that I can size the change before committing.
108. As a maintainer, I want the real migration run to repeat the complete prevalidation, so that a stale preview cannot be applied.
109. As a maintainer, I want migration failures to report per-target outcomes, so that I know exactly which files failed.
110. As a maintainer, I want migration outside a Git repository to proceed with a warning, so that the tool never pretends a recovery net exists.
111. As a maintainer, I want doctor to report parse errors, duplicate IDs, path conflicts, cycles, and projection gaps without repairing them, so that repair stays my decision.
112. As a maintainer, I want doctor to report a held write lock as an informational finding without waiting, so that I know a write is in progress.
113. As a maintainer, I want validate to review the whole project against the current configuration, so that drift surfaces before it blocks work.
114. As a contributor, I want a scoped local read to work while another file is broken, so that one bad file does not blind me to the rest.
115. As a contributor, I want global graph queries to fail explicitly when the projection is incomplete, so that I never act on a partial graph.
116. As a contributor, I want an archived-only range selector on every collection query, so that retired work never mixes into active answers.
117. As a contributor, I want show to read an archived item together with its historical relations, so that I can inspect retired work precisely.
118. As a contributor, I want list, ready, and blocked to share one filter vocabulary, so that I learn and script the filters once.
119. As a contributor, I want a structured query form for complex filters, so that scripts avoid building shell flag strings.
120. As a contributor, I want tree and dependency-tree reads bounded by depth, so that I can explore structure without loading everything.
121. As a contributor, I want statistics to report counts by status and category plus ready and blocked totals, so that I can gauge progress without an audit trail.
122. As a contributor, I want a bounded lock wait with a configurable and per-operation override, so that automation controls contention instead of hanging.
123. As a contributor, I want a lock timeout to guarantee that nothing was written, so that a retry is safe.
124. As an operator, I want an owner mismatch to warn without blocking the write, so that the guardrail informs rather than obstructs.
125. As a contributor, I want a concurrent external edit to produce a conflict instead of a merge, so that I can reread and decide.
126. As a project member, I want selecting a node to open its details without losing my place in the tree, so that I can move between overview and detail.
127. As a project member, I want the Web view to exclude archived items, so that the overview stays focused on active work.
128. As a project member, I want dependency overlays to be optional and remembered per view, so that I can switch between structure and blocking.
129. As a project member, I want the Web view to remain usable with no categories configured, so that a brand-new project is still legible.
130. As a maintainer, I want newer or invalid data to be rejected rather than guessed at, so that a downgrade can never corrupt it.
131. As a terminal user, I want CLI work-state colours only when my terminal supports them and never as the only signal, so that state stays readable in pipes, logs, and for colour-blind readers.
132. As a user without Git installed, I want history and diff to succeed with an empty result and an informative finding, so that the tracker remains usable on machines without Git.
133. As a maintainer, I want doctor and validate to report unavailable Git before I run history, so that I learn the environment gap at a diagnosable point.
134. As a Windows user, I want the CLI, the MCP server and the read-only Web view to behave as they do on Linux and macOS, so that a mixed team shares one repository without a second workflow.
135. As a Windows user, I want the write lock to protect the repository exactly as it does on Unix, so that concurrent writes cannot corrupt my work.
136. As a Windows user, I want editing a file with restricted permissions to preserve those permissions, so that an edit never widens who can read a work item.
137. As a Windows user, I want a read-only file to refuse my edit rather than have Waymark clear the attribute, so that the protection I set is honoured.
138. As a Windows user, I want a symbolic link or reparse point in the store to be refused rather than followed, so that a cloned repository cannot redirect Waymark's writes.
139. As a Windows user, I want an unsupported filesystem to be reported instead of silently promised safety, so that I know when the guarantees do not hold.
140. As a Windows user, I want a diff of an untracked work item to give me a patch I can apply, so that the same review workflow works as on Unix.
141. As a maintainer, I want Windows built and tested natively in CI for both supported targets, so that a Windows regression fails in CI rather than in a user's hands.

## Implementation Decisions

### Domain model and persistence

- A work item is the tracked unit. Each work item is one Markdown document: typed YAML frontmatter followed by an optional Markdown body. Work-item files are the sole authoritative data; no other store outranks them.
- A stable ID is immutable, project-unique, and independent of category, title, and path. It is `wm-` plus 12 Crockford Base32 characters. Persist it lowercase and accept it case-insensitively; normalize before comparison. Ordinary creation never accepts a caller-supplied ID. Detect collisions within the project and regenerate before committing; exhaustion is `ID_GENERATION_FAILED`. Duplicate normalized IDs, case variants, multiple files for one ID, and ID/path mismatch are errors; the projection never chooses one arbitrarily.
- Files are named from the persisted lowercase stable ID plus a fixed Markdown extension. Titles and display names never enter filenames. Category path and key changes move files without changing IDs; an existing target, a case-equivalent collision, path overlap, or an ID/path mismatch is an error and is never overwritten or inferred from the title.
- Categories are project-defined groupings. A category key is project-unique lowercase ASCII, starting with a letter and continuing with lowercase letters, digits, or hyphens; it is separate from the display name. Work items persist the key. Display-name changes never move files. Key and path changes are explicit migrations. Removing a category that still has items is rejected until those items are reassigned. Categories carry no hierarchy, lifecycle, or workflow semantics, and there are no category-specific field schemas.
- No fixed tickets directory layout is assumed. The storage root and every category path are configurable and repository-relative.
- The persisted v1 frontmatter fields are exactly these. The Markdown body is document content after the frontmatter, not a frontmatter field.

| Field | Type | Presence | Mutability | Derived | Persisted |
| --- | --- | --- | --- | --- | --- |
| `id` | stable ID | required | immutable | no | yes |
| `title` | single-line string | required | title edit only | no | yes |
| `category` | category key | required | category move or key migration only | no | yes |
| `status` | `open`, `in_progress`, or `closed` | required | status operation only | no | yes |
| `owner` | identity string | required | creation and owner transfer only | no | yes |
| `creator` | identity string | optional | immutable | no | yes |
| `created_at` | UTC RFC 3339 timestamp | required | immutable | no | yes |
| `updated_at` | UTC RFC 3339 timestamp | required | every effective change except archive | no | yes |
| `closed_at` | UTC RFC 3339 timestamp | optional | status transitions only | no | yes |
| `parent` | stable ID | optional | dedicated parent operations only | no | yes |
| `dependencies` | list of stable IDs | optional; absent means empty | dedicated dependency operations and archive cleanup | no | yes |
| `soft_links` | list of stable IDs | optional; absent means empty | dedicated soft-link operations and archive cleanup | no | yes |
| `custom_fields` | map of declared keys to typed values | optional; absent means none | custom-field edit and schema migration | no | yes |
| `archived_at` | UTC RFC 3339 timestamp | optional | archive only | no | yes |
| `archive_reason` | free-form string | optional | archive only | no | yes |
| `schema` | unsigned integer format generation; current value `1` | required | version migration only | no | yes |

- The following values are computed from the projection and are never persisted: `ready` (boolean), `blocked` (boolean), `blocked_by` (list of stable IDs, including unresolved targets), `children` (list of stable IDs), `dependents` (list of stable IDs), archive state (derived from `archived_at`), and relationship diagnostics. They are not writable through any interface.
- Canonical serialization is strict and deterministic. Emit only the declared fields, in one fixed key order, with one canonical scalar representation. Relationship lists are sorted by stable ID and deduplicated. Unset optional fields are omitted, never written as `null`. Absent relationship lists mean empty lists; absent custom fields mean no custom values. A non-empty body ends in exactly one newline; an empty body has zero length; leading and internal body whitespace is untouched. Re-serializing an unchanged work item is byte-identical.
- Every single-document write is a validated, atomic replacement of the whole file, using a temporary file and rename. No partial document is ever visible at the target.
- `created_at` and `updated_at` are set on creation. Every effective ordinary content or metadata change updates `updated_at`; archive is the explicit exception. `closed_at` is set when status becomes `closed`, cleared when it leaves `closed`, and describes only the most recent close transition; it is not an audit record. Toggling between two non-closed statuses does not touch it.
- Persisted timestamps are UTC RFC 3339 values generated by Waymark. The local user configuration declares a time zone used for human-readable display and for interpreting local time input that carries no offset. Offset-bearing input retains its instant regardless of that time zone. Machine responses default to UTC and may include configured-zone display values where defined. Changing the effective time zone never rewrites work-item files and never changes business timestamps.
- Callers cannot supply `id`, `created_at`, `updated_at`, `closed_at`, `archived_at`, or the format generation. Those are system-generated or operation-managed. A separately specified historical migration or import path may transform legacy times; ordinary user APIs may not. External invalid time values are diagnosed, never silently repaired.
- Fingerprints: a content fingerprint is authoritative for conflict detection and cache invalidation. Path inventory, file size, and modification time are cheap signals; when they change but the content fingerprint is equal, the parsed projection entry may be retained after refreshing metadata, and only a content change triggers semantic re-import. A conflicting write reports the expected and actual fingerprints.
- Unknown, malformed, or undeclared frontmatter keys are locatable validation errors. A malformed file is never silently dropped, rewritten, or guessed at.
- Identity strings for owner, creator, and operator are trimmed at the surrounding edges, otherwise preserved, and compared exactly with no case folding and no resolution to system accounts.
- Physical deletion is not an ordinary operation.

### Configuration

- Two configuration documents define effective behaviour. The shared project configuration at a fixed location inside the Waymark metadata directory defines the project and is the only authority for project structure; the local user configuration in the same metadata directory is an optional, ignored document holding one collaborator's operator identity, time zone, and cache settings. Shared project structure can never be set from the local document.
- The shared configuration carries a version written `major.minor`, meaning format generation and configuration revision.
- The local document is ignored by the metadata directory's ignore file and is never part of the shared baseline. Its absence is normal: every local setting has a fallback, and reading never creates it.
- First initialization writes the local document with the effective Git `user.name`, the current machine-local zone, and the current cache defaults, so a new project is usable without further setup. A missing Git name leaves the operator unset. Initialization options may seed those values only when the document is being created. Initializing an already-initialized project is a no-op: it neither creates nor overwrites the local document, even when seeds are supplied.
- Effective values resolve local-first: an operation-level explicit operator override, then an explicit local value, then the local fallback. The operator fallback is Git's effective `user.name`, the time-zone fallback is the current machine-local zone, and the cache fallback is the built-in defaults. An absent or unreadable Git identity leaves the operator unset rather than inventing one, and an operation continues without it; an undetectable machine zone falls back to UTC with an informational warning, so the result stays deterministic and visible.

Shared project configuration:

| Setting | Value | Presence | Changed by |
| --- | --- | --- | --- |
| format generation (major) | generation declared by the installed Waymark release | required | version migration only; no configuration edit can change it |
| configuration revision (minor) | monotonic counter | required | Waymark-mediated shared configuration writes only |
| storage root | repository-relative directory containing category directories | required | storage migration only |
| categories | set of stable keys with repository-relative paths and display names | required; may be empty | category operations, with path and key changes through migration |
| custom-field declarations | declared keys with type, required or optional, and an optional static default | required; may be empty | optional-field addition and metadata changes through configuration; removal and required-field addition through migration |

Local user configuration:

| Setting | Value | Presence | Changed by |
| --- | --- | --- | --- |
| operator | advisory identity of whoever runs mediated operations | optional; falls back to Git's effective `user.name`, then unset | local configuration operation |
| time zone | named zone used for display and for parsing offset-less local input | optional; falls back to the machine-local zone, then UTC | local configuration operation |
| cache settings | bounded LRU sizing and enablement | optional; falls back to the built-in defaults | local configuration operation |

- Configuration validation rejects, with `CONFIG_INVALID` and a per-issue list. For the shared document: malformed or unparseable configuration; unknown keys; a category key that breaks the key grammar or duplicates another; a category path that is absolute, escapes the repository root, overlaps another category path, is duplicated, or traverses a symbolic link; an empty display name; a custom-field key that is not lowercase ASCII `snake_case`, duplicates another, or shadows a reserved core or computed name; an unknown field type; a default that does not conform to its declared type; and a version that cannot be parsed. For the local document: malformed or unparseable configuration; unknown or shared project keys; an empty operator value when the key is present; an invalid time zone; and an invalid cache value. Each issue is located at the document that carries it.
- Persisted storage-root and category paths, and storage-migration targets, use a portable project-relative syntax on every platform rather than being accepted according to the host platform: `/` separates components; absolute paths, Windows drive-relative forms such as `C:work`, backslashes, empty components, and `..` components are rejected rather than normalized; standalone `.` is allowed only as the storage root and rejected as a category path; embedded `.` components are rejected rather than silently collapsed. Validation never trims, rewrites, or reinterprets the supplied value, and it reports the actual input.
- Components are rejected on every platform when they contain `<`, `>`, `:`, `"`, `\`, `|`, `?`, `*`, any Unicode general-category `Cc` control, or a trailing ASCII space or dot; when the device stem, compared case-insensitively for ASCII letters, is `CON`, `PRN`, `AUX`, `NUL`, `CONIN$`, `CONOUT$`, `COM0`–`COM9`, `LPT0`–`LPT9`, or `COM`/`LPT` followed by a superscript `¹`, `²` or `³`, whether or not an extension follows; or when a device stem is followed by a space before an extension. Unicode names are otherwise allowed. These rules are conservative product policy, not a claim about every Windows naming edge case.
- A category path may not equal or lie inside the project metadata directory `.waymark` or the project's `.git`, and the storage root may not be either of them; the comparison uses the same conservative caseless rule. Standalone `.` remains a valid storage root.
- Path comparison is a fixed-version Unicode comparison rather than ASCII case folding: each component is normalized with `NFD(full_default_case_fold(NFD(component)))` under Unicode 16.0.0, and component sequences are compared for equality and for ancestor/descendant overlap for conflict detection. Original names are preserved. This is a conservative, platform-independent check; it is not a complete detector of filesystem aliases, so operations that touch real paths must also inspect the existing objects.
- An existing configuration that violates these rules is never silently renamed, rewritten, or written through. The specific key, path, and reason are reported as configuration diagnostics scoped to configuration rather than claimed as a full project check; the user backs up, stops writes, coordinates the directory and configuration change, and revalidates. Changing configuration alone can hide existing work-item files, and the migration command is not a configuration repair tool.
- A Unicode data upgrade is an explicit compatibility change: a newer release validates existing configuration with its new comparison rules, reports concrete collisions, and requires manual repair rather than renaming or merging. Release notes warn collaborators to coordinate versions, because two releases may judge the same path pair differently.
- Platform and filesystem length limits are reported explicitly when a path fails; no universal cross-platform length limit is claimed, and a path is never truncated or automatically renamed.
- A configuration whose major generation is newer than the installed release is rejected as `UNSUPPORTED_VERSION`; one older than the immediately previous readable generation is rejected the same way. Configuration inconsistency makes the projection incomplete and prohibits all mediated writes, archive, migration, global statistics, and cross-file relationship writes. Valid work-item files remain locally readable. Waymark never guesses, downgrades, discards unknown data, or repairs it.
- Configuration writes never read or rewrite work-item files. The only work-item read anywhere in the configuration surface is a read-only reference check that refuses removal of a category still in use, and it never modifies an item. Any operation that must change work-item data is a migration. Consequently shared configuration owns: category addition and removal of an unused category, category display-name changes, addition of an optional custom field, and relaxing field metadata. Local configuration owns the operator, the time zone, and the cache settings. Migration owns: storage root changes, category path changes, category key changes, removal of a custom-field declaration, and addition of a required custom field. The surfaces share the category and field nouns but keep separate verbs.
- Adding an optional custom field is a configuration operation and rewrites no work-item file; the field stays unset on existing items. Adding a required custom field is a migration taking a default, because no existing item may be left schema-invalid. Removing a declared field is a migration, not a configuration operation.
- Configuration writes use structured operations with complete validation, fingerprint conflict detection, and one atomic replacement under the project lock. There is no unrestricted raw patch and no partial configuration change.
- The configuration revision increments by exactly one when a Waymark-mediated write to the shared configuration actually changes its content, and does not increment for a no-op. A write to the local document never moves the shared revision, because it changes no shared content. Hand edits do not move it. A version migration that rewrites configuration counts as exactly one configuration write.
- The revision is informational. Freshness, projection invalidation, conflict detection, and cache invalidation are determined by file fingerprints, never by the revision counter; hand-edited configuration is detected by fingerprint regardless of the counter.
- A configuration change that affects the storage root, category keys or paths, or the custom-field schema invalidates the current projection and requires rediscovery and validation. An operator change immediately affects the advisory owner check. A cache-only change clears evictable cache data without invalidating the static projection.

### Lifecycle, relationships, and archive

- Status is exactly `open`, `in_progress`, or `closed`. Any explicit transition among them is allowed for an active item. Repeating the current status changes nothing. A status change updates `updated_at`; entering `closed` sets `closed_at`; leaving `closed` clears it. A closed item is never blocked and no longer blocks its dependents.
- Creation opens the item in status `open`. Status is not a creation argument and is changed only through the status operation.
- `blocked` is a derived condition, never a writable status and never persisted. A non-closed item is blocked when it has an unfinished or missing hard dependency. `blocked_by` is the computed list of unfinished and missing hard dependencies and is never persisted. `ready` is derived and means a non-archived, non-closed item with no unfinished or dangling hard dependency.
- Each item has at most one parent. Parentage may cross categories and is independent of hard dependencies: it does not block, does not imply a dependency, and does not cascade lifecycle. Parent cycles are rejected at write time. A parent cycle introduced by an external edit is diagnosed by `doctor`, not repaired.
- Hard dependencies are directed and block readiness until the prerequisite is closed. Dependencies and soft links are persisted as unique, deterministically sorted lists of stable IDs. Duplicates, self-references, and wrong-kind relationship requests are rejected with `INVALID_RELATIONSHIP` and a reason in details. Dependency cycles are rejected at write time with `CYCLE_DETECTED` and the cycle path. Missing dependency targets block readiness and are reported as dangling.
- Soft links are symmetric and non-blocking. Adding or removing one updates both stored sides in one operation, and storage never leaves a one-sided link. Soft links do not participate in lifecycle cycle rules.
- Relationship mutations are dedicated validated operations, never generic field patches. Multi-target requests prevalidate and commit atomically; a single invalid target fails the whole request. New relationships cannot target an archived item; an archived target is rejected with `ITEM_ARCHIVED`. Relationship writes update `updated_at` on every file they change.
- Archive is an irreversible-through-Waymark logical deletion, distinct from `closed`. It is a mediated write, not a lifecycle transition. It is allowed from `open`, `in_progress`, or `closed`, and it changes neither `status` nor `closed_at`.
- Archiving preserves the original Markdown file and all existing metadata, including body, custom fields, `parent`, `dependencies`, and `soft_links`. It adds `archived_at` and an optional `archive_reason` in the same frontmatter, records no `archived_by`, and does not update `updated_at`.
- Only the current owner initiates archive, subject to the advisory operator check. Archive may affect files owned by other people; only the archived target's owner is checked. Re-archiving returns `ALREADY_ARCHIVED` and replaces neither `archived_at` nor `archive_reason`.
- An item with any unarchived child is rejected with `HAS_ACTIVE_CHILDREN` and the child list. Archive never cascades. Users must archive children individually, clear their parent, or move them first.
- Archived items reject every later mediated write: title, body, custom fields, status, owner transfer, relationships, and re-archive. Such writes return `ITEM_ARCHIVED` with the attempted operation.
- Archive removes recognized inbound core relationships from active files: inbound `dependencies`, inbound `parent`, and inbound `soft_links`. Affected active files update `updated_at`; the archived target does not. Archive never inspects or edits body mentions or custom-field values.
- Archive requires a complete, fresh project projection, performs full prevalidation under the project lock, writes the target and every affected file as one application-level batch, and publishes a new projection only after success. Fingerprint conflicts, an incomplete projection, parse errors, or write failures prevent the operation and change nothing; a rollback failure is reported explicitly with recovery state.
- `archive --dry-run` performs the same completeness, ownership, child, relationship, and conflict checks, reports the target and every planned relationship removal, and writes nothing.
- Archived items are excluded from active tree, list, search, ready, blocked, and statistics results. `show <id>` still reads a single archived item and reports its archive metadata and its own relationship fields as historical relations. An explicit archived-only range selects archived items and never mixes them into the active graph.
- An archived item's own relationship fields remain as historical context and do not participate in active graph calculations. Archive results carry structured changed-item details. No independent archive or audit log exists; Git history and diff remain the durable audit trail. Large archive batches require explicit resource protection and never silently truncate.
- Waymark provides no restore or unarchive command. Restoring means checking out a historical Git version that predates the archive marker; current frontmatter alone determines whether an item is active or archived. Reintroduced archived targets and old relationships are diagnosed from current content.

### Accountability and writes

- Owner is required human accountability. Creator is optional, immutable provenance assigned at creation and never changed afterward. Executor is project-defined metadata, for example a custom field, and is not a core field. None of the three is interchangeable.
- Creation may assign an owner other than the creator. Creation requires title, category, and owner; none of those three is defaulted from configuration. An omitted creator defaults to the effective operator when one is available, and stays unset when none is. A missing required custom field fails the whole create. Creation may include initial parent, dependencies, and soft links; the complete request validates IDs, category, target existence and active state, self-reference, duplicates, cycles, and atomic persistence before writing, so an invalid initial relationship fails the whole create. A create with initial relations requires a complete, fresh projection.
- Only the current owner initiates an owner transfer as a process rule. Transfer takes effect immediately, updates `updated_at`, rejects empty or whitespace-only identities, and is forbidden for archived items. The previous owner loses mediated-write authority at once. Without an authenticated identity, a transfer initiated under a mismatched operator proceeds with an `OWNER_MISMATCH` warning rather than a refusal.
- The configured operator is optional and advisory. It names who is running a Waymark-mediated operation. Every mediated write compares it with the target item's current owner and emits an `OWNER_MISMATCH` warning when they differ; the write is never blocked by the mismatch. This is a workflow guardrail, not authentication: users can edit files and configuration directly. Executor metadata grants no authority.
- Waymark exposes no cross-work-item batch command. Each write targets exactly one work item; there is no multi-ID form, no `items` array, and no mixed-operation array.
- A single operation that inherently writes several files — archive relationship cleanup, migration, multi-target relationship writes, and configuration or schema changes — is application-level all-or-nothing. It prevalidates every target, takes the project lock once, stages all writes, replaces files, and on failure rolls back already-replaced files while retaining recovery material. This is not a filesystem multi-file transaction. If rollback cannot complete, the operation reports `ROLLBACK_FAILED` and never claims success. Every affected path is classified into exactly one of three states — confirmed restored, not restored with a known final state, or final state unknown — and the report also names the location of any recovery material that actually exists and why it needs inspection. An unknown final state is never folded into either of the other two states, and human-mode CLI output lists the affected paths and their states rather than only counts.
- Writing an existing work-item target that is a symbolic link or any Windows reparse point is refused; Waymark never writes through it or replaces its target. When the target's type cannot be established, the write is refused rather than assumed regular. A separate check performed before the write is not sufficient on its own.
- Ordinary single-file writes are staged under the project lock with optimistic content-fingerprint conflict detection. A changed target is rejected with `WRITE_CONFLICT` for reread and retry; there is no automatic merge and no silent overwrite.
- Projection refresh is derived work. After a successful write, a refresh failure leaves the documents committed, marks the projection incomplete and recoverable, and reports that synchronization needs recovery rather than reporting partial success.
- Creating a work item never overwrites an existing target, including an object another process created after validation. The create fails with a conflict and leaves that object untouched; a precheck or the cooperative lock alone is not sufficient, because the lock excludes only cooperating Waymark processes. When the platform cannot provide a no-overwrite creation, the create is refused instead of downgraded.
- Creating a work item sets the stable ID, `created_at`, and `updated_at`; callers cannot provide them. Generic edit cannot change `id`, `creator`, or `created_at`; `closed_at` is maintained only by status transitions; archive metadata only by archive; relationships only by dedicated validated operations or archive cleanup. External violations are diagnosed, not silently repaired.
- Priority, due date, and assignee are not core fields; use project custom fields for project-specific metadata.

### Custom fields and queries

- Custom fields are project-declared, uniquely named data with no field-specific behaviour. A declaration fixes the key, the type, whether the field is required, and an optional static default.

| Declared type | Value shape | Notes |
| --- | --- | --- |
| string | text | any string, including the empty string |
| integer | whole number | decimal integer syntax on the CLI |
| boolean | true or false | no other literals |
| list of scalars | homogeneous array | one declared element type; strings, integers, or booleans |

- Enum is a string convention, not a runtime type. There is no reference type, timestamp type, object type, or workflow type in this version. Nested objects, nested lists, and mixed lists are invalid.
- Field keys are lowercase ASCII `snake_case`. Core and computed names are reserved. Keys are unique within the project.
- Fields are optional by default; a declaration may mark one required. An optional static default may be applied at creation. Unset values are omitted; `null` is invalid. A required field must carry a valid value on every non-archived item; clearing a required field is rejected.
- Undeclared keys and invalid value shapes are rejected as locatable `VALIDATION_ERROR` issues, never silently dropped. A missing required custom field fails the whole create.
- Custom values carry no reference resolution, authorization, or workflow semantics. A stable ID inside a custom value is not a relationship and is not diagnosed or rewritten as one.
- The query vocabulary is finite: presence (`exists` or `missing`); equality and inequality; ordering comparison for strings and integers; case-sensitive prefix and substring matching for strings; whole-list equality and inequality; and explicit element `contains`. There are no arbitrary expressions, nested paths, or nested query semantics.
- Query operands must match the declared field type. There is no implicit conversion; a mismatch is a strict type error.
- Only strings and integers sort. Booleans and lists do not. Missing values sort after present values in both ascending and descending order, and the stable ID breaks ties.
- Field projection accepts either a named set of fields or all set fields. Unknown names return a structured error and unset fields are omitted. The accepted names are exactly `schema`, `id`, `title`, `category`, `status`, `owner`, `creator`, `created_at`, `updated_at`, `closed_at`, `parent`, `dependencies`, `soft_links`, `custom_fields`, `archived_at`, `archive_reason`, `needs_migration`, `ready`, `blocked`, `blocked_by`, `children`, and `dependents`; individual custom-field keys are not projection names, and `custom_fields` selects the whole map. An empty list is rejected, `--fields` conflicts with `--compact`, and `id` and `status` are always present in a projected row.
- Collection queries are active-only by default. An explicit archived-only range selects archived items; there is no implicit active-plus-archived collection. `show` is the explicit single-ID read exception.
- Structured list and query reads frontmatter and custom fields and never the body. Search reads title, frontmatter, and body, follows the same archive-range rules, matches case-insensitively, ranks title hits first, and returns no snippets. The cheap list text filter matches only ID and title and never reads the body.
- `list`, `ready`, and `blocked` share one filter set, one sort vocabulary, and one pagination model: status, category, owner, custom-field predicates, cheap text, parent, leaves-only, and archive range, with sort field, descending flag, limit, offset, projection, and compact output. Default ordering is `rank`: status group (`open`, then `in_progress`, then `closed`), then dependency topological order, then stable ID; the final tie-break within a topological level is the stable ID ascending, in ascending and descending sorts alike (only the primary key reverses).
- CLI `limit` defaults to 100 and MCP `limit` defaults to 50; `0` means unlimited; `offset` skips whole results. Pagination is snapshot-local, not a cross-call cursor. A page cannot skip or repeat items within one snapshot.
- `query` is the structured form of `list` and exists only in the CLI, because the MCP list request is already structured.
- `tree` reads the hierarchy from an optional root item to a bounded depth; `tree --depth 1` covers children and `list --leaves` covers leaves, so neither has a separate command. `dep tree` reads the dependency tree of one item to a bounded depth, with a flat form.
- `stats` reports counts only: total, by status, by category, ready, and blocked, plus an optional distribution over one string field. It reports no throughput or historical snapshots, always states its archive range and projection completeness, and requires a complete, fresh projection.
- Results carry deterministic ordering, stable-ID tie-breakers, and metadata describing source, completeness, freshness, and diagnostics.
- Deferred or malformed data still appears in results where the operation is a local read; global graph operations fail explicitly instead of returning a partial graph.

### Editing

- Ordinary editing is split by data area into three domain operations with matching CLI surfaces and MCP tools: set title, replace body, and update custom fields. There is no generic frontmatter patch and no arbitrary raw document patch.
- Title is full replacement. Trim surrounding whitespace, require non-empty and single-line, and preserve internal whitespace. There is no patch, append, or regex form.
- Body is full replacement. Absent body input leaves the body unchanged during an edit; an explicit empty string or empty body file clears it; a present non-empty value fully replaces it. A non-empty body is canonicalized to exactly one trailing newline without trimming leading or internal whitespace. Body content is never interpreted as a relationship language.
- Custom-field updates take an explicit change map. Each entry is exactly `set` with a value or `clear`; multiple changes may appear in one operation. Null values, unknown keys, duplicate keys, a key appearing in both directions, and clearing a required field are rejected. The whole change set is validated before a single write.
- A CLI field update may mix repeated set and clear arguments; duplicate or conflicting keys are rejected. CLI values are resolved against the declared schema: integers use decimal integer syntax, booleans accept only `true` or `false`, lists require a JSON array, and strings accept an unambiguous token or a JSON string literal. Complex values use canonical JSON.
- MCP takes structured values and an explicit required change map with set and clear entries; it rejects null and never launches an editor.
- Each operation changes only its own data area plus `updated_at` when the effective value changes. An unchanged request performs no write and no timestamp update.
- Every edit rejects archived items with `ITEM_ARCHIVED`, emits an `OWNER_MISMATCH` warning when the operator differs from the owner without blocking the write, uses fingerprint conflict detection, and performs one atomic document replacement per target.
- There is no interactive terminal editor in this version. The CLI accepts direct body text or a body file, mutually exclusive; MCP accepts structured body input.
- MCP schemas are frozen. `waymark_set_title` takes `{id, title, dry_run?, lock_timeout_ms?}` and returns `{id, path, changed, written, item}`. `waymark_replace_body` takes `{id, body, dry_run?, lock_timeout_ms?}`, where `body` is required, an empty string clears it, and there is no "leave unchanged" form. `waymark_update_custom_fields` takes `{id, custom_fields: {set: {key: value}, clear: [key]}, dry_run?, lock_timeout_ms?}` and returns `{id, path, changed, written, item, changed_fields}`; `set` and `clear` are two fields, not a tagged union, and null, unknown, duplicate, conflicting, and required-field clears are `VALIDATION_ERROR`.
- The three edit operations return the complete resulting work item plus a `changed` flag; custom-field updates also return the changed field names. Editing is single-target, with no batch variant, no implicit multi-ID form, and no mixed-operation array.
- There is no composite edit request. Changing title, body, and fields together is three separate calls, each atomic per document with its own lock, fingerprint check, and `updated_at`; there is no cross-area transaction and no all-or-nothing guarantee across the calls, and a partial failure leaves the earlier calls applied. Creation remains the one request that sets several areas at once, and it is atomic because it is a single document write plus its soft-link partners in one batch.
- Only title, body, and custom fields are editable through ordinary editing. Status, owner, category, parent, dependencies, soft links, and archive have dedicated operations. System and identity fields are never editable.

### Interfaces and machine contracts

- Every domain rule lives in one shared function library. The CLI and the MCP server are adapters over it and cannot diverge. Any future interface reuses it.
- CLI and MCP have equal capability coverage. Neither is a reduced surface. Dry runs, complete prevalidation, locks, conflict handling, and structured errors are shared rather than adapter-specific. The adapters may differ only in interaction and transport representation. Capability coverage includes project initialization, configuration reading and writing, validation and diagnostics, work-item creation, reading, editing, moving, and archiving, lifecycle and relationship operations, collection and tree queries with explicit archive ranges, custom-field queries, readiness and blocked views, statistics, migration, Git history and diff where available, and generated agent documentation.
- Project-maintenance capabilities are not split by adapter: initialization, configuration inspection and validation, diagnostics, migration, and Git history and diff access are eligible for both. Projection synchronization is an internal shared-library behavior performed automatically per operation, not an adapter surface. There is no import or export.
- Waymark is a general-purpose tracker. Wayfinder-specific map, decision-ticket, claim-prompt, and frontier semantics do not belong in its core. Generic tree and leaf queries do; a leaf is a work item with no children.

CLI command catalog:

| Command | Arguments | Result |
| --- | --- | --- |
| `init` | project context; optional operator and time-zone seeds | configuration, metadata, and a local document created on first initialization; a no-op when already initialized |
| `project info` | none | storage root, generation, revision |
| `config show` | none | shared and local configuration, effective values, and per-field sources |
| `config validate` | none | validation findings for both documents |
| `config set` | scalar name and value (operator, time zone, cache settings) | the local document updated; the shared revision unchanged |
| `config add category` | key, path, display name | category added |
| `config set category` | key and display name | display name changed; files untouched |
| `config remove category` | key | category removed, or refused while items use it |
| `config add field` | key and type | optional field declared |
| `config set field` | key, attribute, value | field metadata updated |
| `validate` | none | project validation report |
| `doctor` | none | diagnostic findings, including the held-lock probe |
| `migrate version` | target generation, optional dry-run | affected and rewritten files |
| `migrate schema` | remove-field, or add-required-field with default; optional dry-run | affected file list and count, configuration change |
| `migrate storage` | root, or category path, or category key; optional dry-run | moved directories and rewritten category values |
| `git history` | history parameters | commit history for work items |
| `git diff` | diff parameters | work-item diff |
| `show` | stable ID | full item, relationships, derived readiness, archive metadata |
| `list` | shared filters, sort, limit, offset, projection, compact, archive range | page of items with metadata |
| `ready` | same filters as list | page of ready items with metadata |
| `blocked` | same filters as list | page of blocked items with blocked-by |
| `query` | structured filter, sort, limit, offset, projection, compact | structured filter page |
| `search` | text, sort, limit, offset, projection, compact | title and body matches, title hits first |
| `tree` | optional root ID and depth | hierarchy tree |
| `dep tree` | item ID, depth, optional flat form | dependency tree |
| `stats` | optional top-N and archive range | counts only |
| `create` | title, category, owner, optional creator, body or body file, custom fields, parent, dependencies, soft links | created item |
| `title set` | item ID, title | updated item and changed flag |
| `body replace` | item ID, body text or body file | updated item and changed flag |
| `field update` | item ID, repeated set and clear entries | updated item, changed flag, changed fields |
| `status set` | item ID, status | updated item |
| `owner transfer` | item ID, new owner | updated item |
| `move` | item ID, target category | moved item |
| `parent set` | item ID, parent ID | updated item |
| `parent clear` | item ID | updated item |
| `dependency add` | item ID, blocking ID | updated item |
| `dependency remove` | item ID, blocking ID | updated item |
| `link add` | item ID, other ID | both items updated |
| `link remove` | item ID, other ID | both items updated |
| `archive` | item ID, optional reason, optional dry-run | archive result with changed items |
| `version` | none | release version and the capabilities compiled into this build; with `--json`, also the embedded frontend manifest for a Web build |
| `serve` | port, optional open | the read-only Web view on loopback, or the registered `WEB_NOT_INCLUDED` in a basic build |

- `version` and `serve` exist in both build variants. `serve` binds loopback only, never falls back to another port, and reports readiness before it blocks; the Web view it serves is this specification's read-only view, not a second implementation.
- `start`, `close`, and `reopen` may exist as CLI conveniences over the status operation; they are not separate domain capabilities, and MCP exposes the structured status operation rather than one tool per alias. Whether these aliases exist is a command-tree presentation choice.
- Human output keeps a stable text layout: collections render `id`, padded `status`, `state`, and `title` on one line, with indented `needs migration` and `blocked by …` lines and an `N of M match (offset X, limit Y)` footer; a single item renders labeled blocks; edits render one confirmation line; diagnosis, tree, and stats keep sectioned prose. No header row is added. Human prose goes to stderr; stdout carries JSON, `show --raw` documents, and raw `git diff` patches.
- Colour is added only for a terminal: `--color=auto|always|never`, default `auto`, with `NO_COLOR` honoured. Colour never changes the text and never carries meaning alone. The CLI and Web share the semantic states `ready`, `in_progress`, `blocked`, and `closed` and the same green/blue/red/grey mapping; each medium encodes it itself (ANSI or CSS) and the hex values are not a shared contract. Category identity stays shape-based in the Web and is not colour-coded.
- MCP exposes no separate query tool because its list request is already structured. It exposes no separate children or leaves tool; depth and filters express them. There is no public `sync` command or tool: the projection lives only in memory and is rebuilt or refreshed per operation; a CLI invocation builds one and discards it, while long-lived MCP and Web contexts perform lightweight inventory checks before each call. A refresh failure after a successful write leaves the documents committed, reports that synchronization needs recovery, and the next operation rebuilds. `doctor` and `validate` report completeness and freshness but add no rebuild flag.
- MCP transport is stdio only, and the server advertises tools only: no resources, resource templates, prompts, or subscriptions in v1. Tool names follow `waymark_<verb>_<noun>`; the published names are the contract and renaming one is a breaking change.

MCP tool catalog:

| Tool | Inputs | Result |
| --- | --- | --- |
| `waymark_project_init` | project context; optional operator and time-zone seeds | created project |
| `waymark_project_info` | none | storage root, generation, revision |
| `waymark_config_read` | none | shared and local configuration, effective values, and per-field sources |
| `waymark_config_validate` | none | validation findings for both documents |
| `waymark_config_set` | scalar name and value | the local document updated; the shared revision unchanged |
| `waymark_config_add_category` | key, path, display name | category added |
| `waymark_config_update_category` | key and changed attributes | updated category |
| `waymark_config_remove_category` | key | category removed, or refused while items use it |
| `waymark_config_add_field` | key and type | optional field declared |
| `waymark_config_update_field` | key and changed metadata | updated declaration |
| `waymark_validate` | none | project validation report |
| `waymark_doctor` | none | diagnostic findings, including the held-lock probe |
| `waymark_migrate_version` | target generation, dry-run flag | affected and rewritten files |
| `waymark_migrate_schema` | remove-field, or add-required-field with default; dry-run flag | affected file list and count |
| `waymark_migrate_storage` | root, category path, or category key; dry-run flag | moved directories and rewritten values |
| `waymark_git_history` | history parameters | commit history |
| `waymark_git_diff` | diff parameters | work-item diff |
| `waymark_get_work_item` | stable ID | full item with relationships and archive metadata |
| `waymark_list_work_items` | shared filters, sort, limit, offset, projection, archive range | page of items with metadata |
| `waymark_search_work_items` | text, sort, limit, offset, projection | matches, title hits first |
| `waymark_get_tree` | optional root ID and depth | hierarchy tree |
| `waymark_get_dependency_tree` | item ID, depth, flat flag | dependency tree |
| `waymark_get_ready_items` | shared filters | ready items |
| `waymark_get_blocked_items` | shared filters | blocked items with blocked-by |
| `waymark_get_stats` | top-N and archive range | counts only |
| `waymark_create_work_item` | title, category, owner, optional creator, body, custom fields, parent, dependencies, soft links | created item |
| `waymark_set_title` | item ID, structured title | updated item and changed flag |
| `waymark_replace_body` | item ID, structured body | updated item and changed flag |
| `waymark_update_custom_fields` | item ID, required change map of set and clear | updated item, changed flag, changed fields |
| `waymark_set_status` | item ID, status | updated item |
| `waymark_transfer_owner` | item ID, new owner | updated item |
| `waymark_move_work_item` | item ID, target category | moved item |
| `waymark_set_parent` | item ID, parent ID | updated item |
| `waymark_clear_parent` | item ID | updated item |
| `waymark_add_dependency` | item ID, blocking ID | updated item |
| `waymark_remove_dependency` | item ID, blocking ID | updated item |
| `waymark_add_soft_link` | item ID, other ID | both items updated |
| `waymark_remove_soft_link` | item ID, other ID | both items updated |
| `waymark_archive_work_item` | item ID, optional reason, dry-run flag | archive result with changed items |

Shared business contract:

- A write is not blocked by an operator/owner mismatch; the mismatch is reported as a warning.
- Archived items reject ordinary writes.
- Writes use fingerprint conflict detection and never silently overwrite a concurrent external edit.
- Each single-file update is validated and atomically replaced.
- There are no user-visible cross-work-item batch commands; CLI accepts no multiple IDs and MCP accepts no `items` array, and there is no arbitrary mixed-operation array.
- All-or-nothing and per-target outcomes apply only to a single operation that inherently touches multiple files, and are guaranteed by the shared write engine through prevalidation, the project lock, staging, rollback, and explicit rollback failure.
- System time is generated by Waymark; ordinary requests cannot forge it.
- Machine responses use a versioned envelope.
- Errors use shared stable codes with typed details, location, and retryable.
- The CLI maps shared errors to exit codes; MCP reuses the same codes and does not create a second business-error taxonomy.
- JSON-mode failure envelopes go to stdout so a parser always receives valid JSON; stderr stays human narrative.

Machine envelope and metadata:

- Success is `{v, ok: true, data, _meta}`; failure is `{v, ok: false, error}`. Errors are structurally identical across adapters. The CLI supports human and versioned JSON output; MCP returns the shared envelope.
- Every error carries `{v, code, message, details, location?, retryable}`. `details` is typed per code, not a free-form map. `location` is a tagged union over `work_item`, `file`, `field`, `config_key`, and `relationship`. Messages stay human-readable and are never parsed for classification.
- `_meta` carries source (`direct_file` or `memory_projection`), completeness (`scoped` or `complete`), freshness, diagnostics, and warnings. A warning is `{code, message, location?, details?}`: `location` stays optional and `details` is typed per code through the same registry discipline as error details. Warnings such as `OWNER_MISMATCH` never produce a non-zero exit.
- A successful replacement whose directory synchronization is not confirmed carries a `DIRECTORY_SYNC_UNCONFIRMED` warning with `{directory, paths[]}` in `details` and no `location`; one warning describes one failed synchronization event, which may cover several files. Warning details are pinned by the registry snapshot like error details, so a code cannot acquire an unreviewed detail key.
- Directory-synchronization capability is declared per platform and is not probed per write. An attempt that fails produces the warning above; a platform that provides no directory-entry durability mechanism records that once as a documented platform limitation and does not emit a per-write warning, while the write is still never described as durable after power loss.
- Every path in a machine response is a tagged union: a path that is valid UTF-8 is a plain string, and a path that cannot be represented losslessly becomes an object carrying base64 bytes. The same representation is used uniformly, including inside structured locations, so a consumer never has to guess field by field whether a destination can be an object. Human-readable output renders undecodable bytes as `\xNN` with the readable remainder intact, using no ellipsis and no U+FFFD substitution.
- Waymark's own created paths are always valid UTF-8; only an existing external file or directory that is referenced can carry a non-UTF-8 path. There is no byte-path mode, and a required path that cannot be represented losslessly is never silently replaced and then operated on.
- A diagnostic finding is `{severity: error, warning, or info, code, location?, message, fixable}` and reuses the same registry.

Error taxonomy structure:

- Normal failures are returned as typed, enumerable errors rather than signalled by panics or by human-readable text used as a contract. Unexpected panics are program faults handled at the process boundary.
- One shared registry is the single classification source. Each entry records the stable code, its exit class from 2 through 7, whether the same request is retryable, the typed details shape, the location kinds it may carry, and whether it may also appear as a diagnostic finding. The CLI exit mapping and the MCP business-error representation both read this registry; neither adapter keeps its own match.
- Codes are flat `SCREAMING_SNAKE` strings. Names carry no parsed semantics: exit class and retryability are registry attributes and are never derived from a prefix. `MIGRATION_*` is a naming convention, not a classification rule.
- A code names a recovery unit, not an error variant. A new code is warranted only when the recovery action differs; when only the offending value differs, the difference belongs in typed details. `PARSE_ERROR` and `VALIDATION_ERROR` deliberately cover whole families of conditions. `CYCLE_DETECTED` covers dependency and parent cycles with the kind in details. `INVALID_RELATIONSHIP` covers self-reference, duplicate edges, and wrong relation kinds with the reason in details. `VALIDATION_ERROR` covers field type mismatch, a missing required field, an unknown field, an empty or multi-line title, an invalid status literal, set/clear conflicts, and null values, with a per-field issue list.
- Class 4 is narrowed to conflict and state rejection. Rejecting a write to a stale-generation item is class 6; an incomplete projection is class 3; lock contention and an unstable reader snapshot are class 5.
- Warnings never change the exit code. Usage-level failures stay adapter-level: bad flags and unknown subcommands exit 1 with `USAGE_ERROR` and are absent from the domain registry. MCP JSON-RPC protocol errors stay separate from business errors and cover only malformed requests, unknown tools, and transport failures.
- The registry is pinned four ways: a wildcard-free compile-time match forces every error variant to declare a code; a registry-consistency test keeps the code table and that match in step; a JSON-schema enum round-trip keeps the documented schema honest; and a golden snapshot records each code's exit class, retryability, details keys, and location kinds. Adding, removing, or reclassifying a code therefore requires updating the snapshot and this table in the same change.

Exit classes:

| Exit | Class | Contents |
| --- | --- | --- |
| 0 | success | command completed; warnings and informational findings do not change it |
| 1 | usage | bad flags, unknown subcommands, adapter-level `USAGE_ERROR`; deliberately outside the registry |
| 2 | not found | missing work item or category |
| 3 | invalid input or data | validation, parsing, configuration, IDs, cycles, relationships, incomplete projection |
| 4 | conflict or state rejection | write conflict, archived target, archived-item write, active children |
| 5 | filesystem, Git, or environment | project not found, IO, Git, lock, unstable snapshot |
| 6 | version or migration | unsupported version, item needs migration, action requires migration, migration failures |
| 7 | internal | rollback failure, ID generation failure, internal fault |

Error registry:

| Code | Exit | Retryable | Finding | Location kinds | Typed details | Meaning |
| --- | --- | --- | --- | --- | --- | --- |
| `ITEM_NOT_FOUND` | 2 | no | none | work_item, relationship | `{id, relation?}` | no work item exists for the requested or referenced stable ID |
| `CATEGORY_NOT_FOUND` | 2 | no | none | field, config_key | `{key}` | no declared category has the given key |
| `VALIDATION_ERROR` | 3 | no | error | work_item, file, field, config_key, relationship | `{target, issues:[{path, reason}]}` | one or more field-level validation failures |
| `INVALID_ID` | 3 | no | none | field, file | `{value, reason}` | an ID literal does not satisfy the stable-ID format |
| `PARSE_ERROR` | 3 | no | error | file | `{path, reason}` | a document or its frontmatter cannot be parsed |
| `CONFIG_INVALID` | 3 | no | error | config_key, file | `{path, issues[]}` | configuration fails schema validation |
| `DUPLICATE_ID` | 3 | no | error | file, work_item | `{id, paths[]}` | two or more files claim the same normalized ID |
| `ID_PATH_MISMATCH` | 3 | no | error | file, work_item | `{id, path, stem}` | the persisted ID disagrees with its filename stem |
| `CYCLE_DETECTED` | 3 | no | error | work_item, relationship | `{kind, cycle[]}` | a dependency or parent cycle was found; kind is dependency or parent |
| `INVALID_RELATIONSHIP` | 3 | no | none | work_item, relationship | `{relation, from, to, reason}` | self-reference, duplicate edge, or wrong relation kind |
| `INCOMPLETE_PROJECTION` | 3 | no | error | file, work_item | `{completeness, freshness, diagnostics[]}` | a complete, fresh projection is unavailable |
| `WEB_NOT_INCLUDED` | 3 | no | none | none | `{}` | a basic build was asked to serve the Web view, which it does not compile in |
| `WRITE_CONFLICT` | 4 | no | none | work_item, file | `{id, path, expected_fingerprint, actual_fingerprint}` | the target changed since the operation read it |
| `ITEM_ARCHIVED` | 4 | no | none | work_item | `{id, operation}` | an archived item rejects a write or a write targets an archived item |
| `ALREADY_ARCHIVED` | 4 | no | none | work_item | `{id}` | archive was requested for an already archived item |
| `HAS_ACTIVE_CHILDREN` | 4 | no | none | work_item, relationship | `{id, children[]}` | archive is blocked by unarchived children |
| `PROJECT_NOT_FOUND` | 5 | no | none | none | `{searched}` | no Waymark project was found while searching |
| `IO_ERROR` | 5 | no | error | file | `{path, operation}` | a filesystem operation failed |
| `GIT_ERROR` | 5 | no | none | none | `{operation, reason}` | a Git operation failed |
| `GIT_UNAVAILABLE` | none | no | info | none | `{}` | no `git` executable was found on `PATH`; history and diff degrade to empty |
| `LOCK_TIMEOUT` | 5 | yes | none | none | `{lock_path, waited_ms}` | the lock wait expired; nothing was written |
| `LOCK_FAILED` | 5 | no | none | none | `{lock_path, reason}` | the lock file cannot be opened, locked, or is not a regular file |
| `SNAPSHOT_UNSTABLE` | 5 | yes | none | none | `{retries}` | a reader could not obtain a stable snapshot after bounded retries |
| `PORT_IN_USE` | 5 | no | none | none | `{port}` | the requested loopback port is already bound; there is no fallback port |
| `UNSUPPORTED_VERSION` | 6 | no | error | file, work_item, config_key | `{found, supported}` | data declares a generation the installed release cannot read |
| `ITEM_NEEDS_MIGRATION` | 6 | no | error | work_item, file | `{id, item_generation, project_generation}` | an item's generation is older than the project generation |
| `ACTION_REQUIRES_MIGRATION` | 6 | no | none | work_item, config_key | `{action, required_command}` | the operation requires an explicit migration first |
| `MIGRATION_PATH_CONFLICT` | 6 | no | none | file | `{target, conflict}` | a migration target exists, overlaps, or is case-equivalent to an existing path |
| `MIGRATION_UNSUPPORTED_GENERATION` | 6 | no | none | none | `{requested, supported}` | the installed release cannot write the requested generation |
| `MIGRATION_PRECONDITION_FAILED` | 6 | no | none | file, work_item, config_key | `{diagnostics[]}` | incomplete projection or fingerprint disagreement before migration |
| `MIGRATION_FAILED` | 6 | no | none | file, work_item | `{per_target:[{path, outcome, error?}]}` | migration execution failed; per-target outcomes are reported |
| `ROLLBACK_FAILED` | 7 | no | none | file | `{restored[], failed[], unknown[], recovery_material:[{path, reason}]}` | a multi-file operation partially applied and rollback did not complete |
| `ID_GENERATION_FAILED` | 7 | no | none | none | `{attempts}` | a unique stable ID could not be allocated after bounded attempts |
| `INTERNAL_ERROR` | 7 | no | none | none | `{}` | an unexpected program fault |
| `OWNER_MISMATCH` | none | no | warning | work_item | `{item, operator, owner}` | the declared operator differs from the item owner |
| `LOCK_HELD` | none | no | info | none | `{lock_path}` | a mediated write currently holds the project lock |
| `MIGRATION_AVAILABLE` | none | no | info | work_item | `{items[]}` | one or more items need migration |
| `NOT_A_GIT_REPOSITORY` | none | no | info | none | `{}` | no Git repository is available for history or recovery |
| `DANGLING_REFERENCE` | none | no | error | work_item, relationship | `{from, relation, missing[]}` | a structured relationship references a missing target |
| `TIMESTAMP_INCOHERENT` | none | no | warning | work_item, field | `{id, field, value, reason}` | a persisted system timestamp is internally inconsistent |
| `DIRECTORY_SYNC_UNCONFIRMED` | none | no | warning | none | `{directory, paths[]}` | a replacement succeeded but its directory synchronization was not confirmed |
| `RECOVERY_MATERIAL_FOUND` | none | no | info | file | `{path, reason}` | temporary or recovery material may exist and requires human inspection |
| `DIAGNOSTIC_SCOPE_UNCONFIRMED` | none | no | info | file | `{path, reason}` | a diagnostic location could not be checked, so clean coverage is not claimed |
| `ASSETS_VERSION_MISMATCH` | none | no | error | none | `{kind, expected, actual}` | the embedded frontend manifest disagrees with the release it claims to describe |
| `TIME_ZONE_UNDETECTED` | none | no | info | none | `{}` | the machine-local time zone could not be determined, so the effective zone falls back to UTC |

- `LOCK_TIMEOUT` and `SNAPSHOT_UNSTABLE` are the only retryable codes. Every other code is terminal. `USAGE_ERROR` at exit 1 is adapter-level and deliberately absent from the registry.

### Versioning and migration

- One command-managed generation governs configuration and work-item compatibility. The configuration version is `major.minor`: the major is the on-disk format generation, declared by the installed Waymark release, and the minor is the monotonic configuration revision.
- The format generation changes only when Waymark's own file format changes: the core field set, serialization rules, or body rules. No configuration edit can raise it. The shared configuration revision increments by exactly one per effective mediated shared-configuration write, never otherwise, and never for a local-document write.
- Work-item frontmatter persists only the format generation. It never persists the configuration revision, so a configuration change can never make an existing work item stale. The configuration revision is informational; fingerprints determine freshness, projection invalidation, conflict detection, and cache invalidation.
- Waymark retains a reader for the current and the immediately previous format generation. Older generations are rejected as `UNSUPPORTED_VERSION`. A generation newer than the installed release is reported on local reads as `UNSUPPORTED_VERSION` and makes the project projection incomplete; it never leads to guessing, downgrading, discarding unknown data, or silent repair.
- A work item whose format generation is older than the configured generation is read-only for mediated writes: title, body, and custom-field edits, status changes, owner transfer, relationship operations, and archive are rejected with `ITEM_NEEDS_MIGRATION` or `ACTION_REQUIRES_MIGRATION` until an explicit version migration runs. Reads and `show` remain allowed, and list, tree, readiness, blocked, and stats still include such items, marked as needing migration rather than hidden.
- Ordinary writes never rewrite an older-generation file. Only an explicit migration changes a file's generation. There is no lazy upgrade on ordinary write.
- Every migration kind exposes an explicit dry run that performs the complete read-only check and reports the affected files, and a real execution that repeats the same checks before writing. There is no persisted plan file, plan identifier, or plan/apply handshake; re-derivation at execution time replaces it.
- Durable migration backups are not provided. Recovery after a successful but wrong migration relies on Git or user copies. A single run is still application-level all-or-nothing with in-run rollback from staged copies, and a rollback failure is reported explicitly. Migration is permitted outside a Git repository, with a `NOT_A_GIT_REPOSITORY` warning that Git-based recovery is unavailable.
- There is no separate compatibility-review command: `validate` and `doctor` perform the review against the current configuration, and a migration dry run previews the change.
- Migration uses dedicated error codes because the recovery action differs: `MIGRATION_PATH_CONFLICT` for an existing, overlapping, or case-colliding target; `MIGRATION_UNSUPPORTED_GENERATION` when the installed release cannot write the requested generation; `MIGRATION_PRECONDITION_FAILED` for incomplete projections or fingerprint disagreement. Failures during execution are `MIGRATION_FAILED` with per-target outcomes.
- Every migration preserves archive metadata in frontmatter and the canonical serialization rules; a migrated file remains a valid work-item document under the new generation.
- All three classes return one shared report: `{migration: version|schema|storage, description, dry_run, affected: [{id, path}], targets: [{path, outcome, error?}], moves: [{from, to, outcome, error?}], rewritten: [{id, path, from, to}], count, written, config_changed, previous_revision, revision}`. A dry run sends the same request with `dry_run: true` and returns the same shape with `written: false` and targets marked `skipped`; a real run marks each target `applied` or `failed`. Requests carry exactly one action: version `{target_generation}`; schema `{action: {kind: remove_field, key} | {kind: add_required_field, key, type, default?}}`; storage `{action: {kind: root, path} | {kind: category_path, key, path} | {kind: category_key, from, to}}`; all plus `dry_run` and optional `lock_timeout_ms`. CLI flags map one-to-one.

Version migration:

- Request: a target format generation that the installed release can write, plus a dry-run flag. The user chooses when to migrate, not which generation number to declare.
- A real run re-derives the file set and revalidates every file, rewrites each affected work-item document to the target generation using the canonical serializer, and counts as one configuration write when configuration content changes.
- Result: the affected and rewritten file set, with counts, and per-target outcomes on failure; nothing is written in a dry run.

Schema migration:

- Request: exactly one action — remove a declared custom field, or add a required custom field with a default; plus a dry-run flag. Adding an optional custom field is a configuration operation, not a migration action.
- Removing a field deletes the declaration and the key from every work item, including archived ones, as one application-level operation. The dry run reports the affected file list and count. The real run increments the configuration revision once; a failure leaves configuration and files unchanged.
- Adding a required field supplies the declared default where an existing item has no value, so no item is left schema-invalid. The dry run reports the affected files.
- Changing a field's type, renaming a field, changing a list element type, and making an optional field required are rejected by migration and must be handled as an explicit schema change plus a manual or scripted data edit. Migration never silently drops, guesses, or partially converts values.
- Because removing a declared field leaves existing files schema-invalid, that drift is detected by validation as an undeclared key rather than by any version comparison.

Storage migration:

- Request: exactly one mutually exclusive action — change the storage root and move every category directory; change one category path and move that directory; or change one category key, which also rewrites the persisted category value in every affected work item. Actions are never combined in one run.
- Prevalidation rejects, before any write: absolute paths, Windows drive-relative forms, and paths escaping the repository root; target paths that overlap an existing category path by equality, ancestry, or descent under the conservative caseless comparison; a target that already exists at all, including an empty directory, which is never overwritten or deleted; pre-existing symbolic links and any Windows reparse point, including junctions and mounted folders, in the target path or any component it traverses; source files that are missing, unreadable, or disagree with the projection fingerprint; and an incomplete or stale projection. Any failure aborts the whole migration and reports every diagnostic.
- A migration that would cross filesystem volumes is rejected; copying and deleting is never substituted for a same-filesystem directory move. A successful precheck does not authorize execution: the run repeats the complete prevalidation, and when volume identity cannot be established the dry run reports that uncertainty while actual execution refuses. Volume identity is taken from handles and compared as an identity, not inferred from path spelling.
- Actual execution must prevent a target created by another process after prevalidation from being overwritten. The advisory lock excludes only cooperating Waymark processes, so this relies on a no-replace primitive rather than on the lock; if the platform cannot provide it, the migration refuses.
- The no-existing-target rule applies to each actual category directory being moved, not to an existing storage-root container or a target parent directory that already exists. A migration from `work` to `.` may move `work/task` to an absent `task` under the existing project root, but it never moves, deletes, or replaces the project root itself.
- Rollback never overwrites an object another process created at a moved item's original location. If a move cannot be safely restored, restoration stops at that point and the run reports rollback failure, the affected paths and states, and where any recovery material actually exists, rather than deleting the external object to claim success.
- Parent directories created by the migration are removed on failure only when they can still be confirmed as the same directories this run created and are empty; a matching pathname is not sufficient. Cleanup failures and unknown states are reported rather than claimed as complete restoration.
- A migration is application-level all-or-nothing, not a filesystem transaction: a crash, power loss, or forced termination may leave a partially moved tree, and no automatic crash recovery is promised. Recovery relies on Git or user copies, so outside a Git repository the migration warns with `NOT_A_GIT_REPOSITORY`.
- Category paths may lie outside the configured storage root. A storage-root migration that cannot derive a destination for such a category refuses and names the category rather than leaving it unmoved, moving it somewhere unrequested, or guessing.
- A pure relocation is a same-filesystem directory rename: contents, fingerprints, and `updated_at` are untouched. A category-key migration additionally rewrites the persisted category value and is not a pure relocation.
- Result: moved directories and rewritten values, with per-target outcomes on failure; nothing is written in a dry run.

### Diagnostics and health checks

- A diagnostic operation reports data-integrity findings separately from command execution failure. A warning-only report succeeds by default; strict mode returns the validation failure class (exit 3) when error-severity findings remain.
- Findings reuse the registry and are `{severity: error, warning, or info, code, location?, message, fixable}`. There is no second diagnostic taxonomy. Details are schema-controlled rather than an unbounded string bag, so adding a code or field is a deliberate, tested change.
- `doctor` and `validate` cover: malformed or oversized frontmatter; duplicate IDs, case variants, and multiple files for one ID; ID/path mismatch; invalid fields and undeclared custom fields; missing or archived relationship targets, reported as `DANGLING_REFERENCE`; dependency and parent cycles; unsupported or future versions; stale or invalid projections; source inventory and fingerprint changes, including new, deleted, moved, and uncertain files; path conflicts; projection incompleteness; invalid or malformed local user configuration, located at the local document; timestamp incoherence, reported as `TIMESTAMP_INCOHERENT`; and items needing migration, reported as `MIGRATION_AVAILABLE`. A local-document finding never changes the shared configuration revision and never affects another clone. A Web build additionally reports an embedded-manifest disagreement as `ASSETS_VERSION_MISMATCH`.
- `doctor` is diagnostic-only for business data. It never deletes a relationship, modifies body or custom fields, restores an item, or infers user intent. Cache and projection rebuild is the only safe internal repair, and every repair reports what changed and what remains.
- `doctor` probes the lock with a non-blocking try-lock. When the lock is held it reports `LOCK_HELD` as an info-level, non-fixable finding, never waits, and never reports holder identity.
- `doctor` also probes Git availability and reports `GIT_UNAVAILABLE` or `NOT_A_GIT_REPOSITORY` as info-level, non-fixable findings; it never installs Git or initializes a repository.
- Diagnostics for possible temporary or recovery material are bounded to locations derived from the current configuration — the storage root, each configured category directory, and the project metadata directory — rather than hard-coded platform paths or the system temporary directory, because staging happens in the destination directory. The checked scope and any coverage that cannot be confirmed are reported; a location no longer derivable from configuration, such as a former category directory after an interrupted migration, is reported as uncovered, so finding nothing does not prove that nothing exists.
- Material that may need inspection is reported as `RECOVERY_MATERIAL_FOUND` with its path and the reason it needs inspection. A location that cannot be checked is reported as `DIAGNOSTIC_SCOPE_UNCONFIRMED` with its path and reason; it is never treated as absent, and one unreachable location does not fail the whole diagnostic. Both are read-only, info-level and non-fixable.
- These findings never delete, move, restore, or read through a link, and never infer from a name or an age that material is abandoned; another Waymark process may be using it. They do not make the material authoritative and do not promise automatic crash recovery.
- Cycle detection returns a structured cycle path or diagnostic and never panics. A scan that cannot validate the complete graph fails explicitly rather than validating a partial graph as complete.
- Local reads may succeed for valid files while another file is broken. Global graph queries, archive, migration, and multi-target relationship writes require a complete projection and fail explicitly with `INCOMPLETE_PROJECTION` on parse errors or uncertainty.
- `validate` reviews the whole project against the current configuration and reports findings; it performs the compatibility review that would otherwise need a separate command.

### Projection, cache, and concurrency

- Work-item files are the sole authority. No JSON index or database is persisted.
- A process may hold a complete in-process static frontmatter projection containing parsed core and custom fields, source paths, fingerprints, parse diagnostics, and forward and reverse core relationship helpers. It is disposable and rebuilt from files; it is never a second source of truth.
- Derived values — readiness, blockage, blocked-by, children, dependents, and relationship diagnostics — are computed from the current projection and never persisted.
- A projection is either complete and fresh or explicitly incomplete or uncertain. A scoped local read may parse a file directly without loading it into the long-lived projection, but must identify itself as scoped. The `load_into_memory` strategy controls whether a read updates the long-lived projection.
- Global list, stats, tree, readiness, blocked queries, archive, migration, and cross-file relationship writes require a complete, fresh projection. Local operations may proceed on an explicitly marked partial projection. Partial state is never published as complete.
- `show` may use a direct file read and may temporarily read related frontmatter without recursively loading the whole graph.
- `touch` is an internal term for a Waymark-controlled frontmatter query, frontmatter mutation, or archive. It never means updating a timestamp and never means arbitrary body access. Public interfaces use explicit read, write, archive, and sync names.
- Long-lived processes may retain a projection and perform lightweight inventory checks before operating. New, deleted, moved, changed, or uncertain files invalidate the relevant entry or the whole projection; stale data is never returned as current. CLI may build a projection for one invocation and discard it on exit.
- Fingerprint validation uses path inventory, size, and modification time as cheap signals, then the content fingerprint when needed. A metadata-only change with an equal content fingerprint may retain the parsed entry after refreshing metadata.
- A parse error permits valid local reads but prevents operations that need a complete graph. A global read that meets a concurrent multi-file write retries a bounded number of times, then returns retryable `SNAPSHOT_UNSTABLE` rather than a torn snapshot.
- Writes go to documents first under the project lock with fingerprint conflict checks, then synchronize or rebuild the projection. Incremental synchronization is allowed only when the affected scope is known; otherwise the complete projection is rebuilt.
- A bounded, configurable LRU may cache body, detail, or query-result data for performance only. Entries are fingerprint-bound. It never evicts relationship nodes from a complete projection, and no cache setting may weaken freshness, completeness, ownership checks, or archive safety. Cache-only configuration changes clear evictable data without touching the static projection.
- The project write lock is one store-wide advisory lock on a dedicated file at a fixed location inside the Waymark metadata directory. It is never taken on a work-item file, because those are replaced by temporary file plus rename and an advisory lock on one would bind to a stale inode.
- It is an OS advisory exclusive lock held on an open descriptor for the operation's whole read-modify-write window and released by the kernel when the process exits or crashes. There is no stale-lock reclamation, no PID probe, no modification-time expiry, and no holder metadata; the lock file's content is never read or written.
- The lock file is created by the first mediated write and is listed in the project's ignore file for the metadata directory, as is the local user configuration document. A missing entry is repaired on the next lock acquisition or before a local-document write. The ignore file is opened without following a symbolic link and must be a regular file, so a cloned repository cannot turn it into a link to a user file; an invalid, unwritable, or symlinked ignore file stops a local-document write rather than risking a committed preferences file.
- The lock is non-reentrant. One public operation acquires it exactly once and holds it through prevalidation, every staged write, and rollback. Internal helpers that write receive the held guard instead of acquiring again; nested acquisition is a programming error rather than a wait.
- Every mediated write takes this one lock: work items, relationships, archive, migration, and configuration edits. Reads, `show`, dry runs, and previews never block on it.
- Long-lived adapters never hold the lock between calls. The MCP server and any writer acquire it per mediated write and release it before responding.
- A writer waits a bounded time, 10 seconds by default, configurable per project and overridable per operation through a shared option exposed by both CLI and MCP; zero fails immediately. On expiry it returns retryable `LOCK_TIMEOUT` naming the lock path and waited duration and guaranteeing that nothing was written. No holder identity is exposed.
- Failing to open or take the lock file because of an unreadable path, a read-only project directory, or a non-regular file is a terminal `LOCK_FAILED` operation error. Reads keep working in a read-only checkout because they never need the lock.
- Concurrency failures stay three distinct conditions: lock contention, retryable with nothing written; a fingerprint conflict with an external edit, terminal, requiring reread; and reader snapshot instability during a concurrent multi-file write, retryable and read-only.
- The lock's lifetime follows the project metadata directory, not the storage root; storage migration never moves or recreates it.
- The lock contract applies on every supported platform: Linux, macOS, and Windows x86_64 for both the MSVC and GNU targets. Opening the lock file never follows a final symbolic link and never follows a final Windows reparse point; a lock path that is a link, a reparse point, a directory, or otherwise not a regular file is `LOCK_FAILED`, and an inability to establish the file's type is also `LOCK_FAILED` rather than a guess. A check performed before opening is not sufficient on its own; the decision must be made from the opened handle where the platform allows it.
- A lock file that the operating system reports as having more than one hard link is refused with `LOCK_FAILED`. When the hard-link count cannot be established reliably, the check fails closed instead of proceeding to an unlocked write.
- The project metadata directory itself must be a real directory, not a symbolic link and not a Windows directory reparse point; if it is, mediated writes are refused with an explanation that a regular metadata directory is required.
- A project may be reached through a symbolic link or equivalent alias to its root. Two aliases of the same actual project must contend on the same lock, so the lock is located from the project's real identity rather than from the spelling of the path used to open it.
- Waymark never deletes or recreates the lock file to release the lock, and tells users not to clean it manually. External deletion or replacement of the lock path during a write is unsupported; where the operating system prevents the replacement or Waymark detects the anomaly, the operation fails safely rather than continuing.
- The Windows MSVC and GNU builds use the same lock protocol: simultaneous mediated writes to the same local project serialize, or the contender waits and fails under the same timeout policy. A build that could not interoperate with the other Windows target would be a defect, not a documented difference.
- Repairing a missing ignore entry happens before the lock is acquired and touches only the metadata directory's ignore file; it is not routed through the lock's safe-open path, because it runs before that path exists for the operation.
- The lock is filesystem-local and covers local filesystems only. Concurrent access over network filesystems, including NFS and SMB, is not supported and is not made safe by the lock.

### Platform support

- Supported targets are Linux, macOS on both architectures currently shipped, and Windows x86_64 for the MSVC and GNU toolchains. Windows ARM64, Cygwin, WSL-specific behaviour, and concurrent use over network filesystems are outside this specification.
- Each supported target declares its platform capabilities at build time. Runtime code reports a declared capability that failed rather than probing the platform on every operation, and a capability declared absent fails closed for the operations that require it: there is no silent fallback to a weaker mechanism, and no operation reports success for a guarantee the platform did not provide.
- Capabilities that depend on the actual filesystem rather than on the platform — at minimum volume identity and reparse-point identification — remain per-operation checks even where the platform declares support. A single inconclusive judgement is treated as unknown and the operation refuses.
- Windows support is limited to local NTFS volumes. FAT, exFAT, network shares, and any other filesystem are unsupported and fail closed rather than degrading. Linux and macOS support is not extended by this specification.
- On Windows, replacing an existing file preserves its owner and discretionary access control list, including inheritance and protection state, and refuses the replacement when preservation cannot be established. A file marked read-only refuses the edit and keeps its attributes. Temporary, staged, and recovery files are never more accessible than the target they serve. A newly created file may inherit its destination directory's permissions, and staging never grants broader access than the intended final policy. Preservation of SACLs, alternate data streams, and timestamps is not promised.
- Configured category paths and migration targets refuse pre-existing reparse points, including junctions and mounted folders. Direct work-item reads and inventory scans refuse a final symbolic link or reparse point and never follow it, and an uncertain file type is an error rather than an assumption. A permitted project-root alias remains allowed; defence against an adversary replacing an ancestor during an operation is explicitly out of scope.
- These statements are requirements. No platform is claimed verified until native evidence exists, and an agreed behaviour that proves infeasible on a platform returns for a decision rather than being quietly weakened.

### Read-only Web

- Web is strictly read-only, simple, and an overview rather than a search or filter replacement. It has no write path, no in-page refresh, and no archive browser.
- It renders non-archived work items as top-to-bottom trees: combined by default, or grouped by category, with the selected view remembered.
- No synthetic root is invented: every parentless item is a root. In the grouped view, cross-category children remain under their actual parent.
- Category identity uses stable node shapes plus the category name or abbreviation, cycling stably when categories outnumber shapes.
- Closed items remain visible. State colours distinguish open or ready, in-progress, derived blocked, and closed, with a concise text marker for blocked. Colours are never derived from custom or category-specific values.
- Solid edges show parenthood. Optional dashed dependency edges have a simple global show or hide control remembered separately per view. In the grouped view, a displayed item depending on another category adds a reference node and a dashed edge, anchored to the referenced item's parent when it has one; hiding dependencies also hides those reference nodes.
- Selecting an item opens a read-only detail view with body, category, actual status, blocked reason, owner, existing custom fields, and relationships. Custom-field names are not assigned special semantics. Navigation preserves the viewer's place in the tree.
- Per-node and whole-tree expand and collapse are supported, with expansion state kept separately for each view.
- Selected view, per-view expansion, per-view dependency visibility, and scroll position persist in the current browser only, isolated by project at a given local service origin. A local identifier derived from the normalized project-root path distinguishes projects; it is not uploaded or written to project files. Moving a project changes that identity and starts with defaults. Different browsers, hosts, schemes, or ports may start with defaults because browser storage is origin-scoped; no shared service or project file synchronizes preferences. If storage is unavailable or saving fails, browsing and view controls remain usable for the session and a concise notice explains that preferences cannot persist. Web never writes project data or preferences into the repository.
- Narrow screens keep the vertical tree with compact indentation, wrapping, and vertical scrolling. An empty project shows a concise empty state, and the combined view remains usable with no categories configured.
- In a complete inventory, invalid or missing parent references do not suppress valid trees; unattached items appear in a clearly explained unresolved-relations area rather than being hidden or treated as ordinary roots. If the shared core cannot supply a complete tree because a file is damaged or the projection is incomplete, show a read failure and available diagnostics instead of presenting a partial tree as complete.
- Use Axum and Tokio for the Web backend and SvelteKit, Svelte, TypeScript and Vite for the frontend; use existing HTTP and Markdown libraries rather than implementing HTTP by hand. Select versions after verifying the complete Rust dependency graph against the supported Rust version; report incompatibilities for a decision rather than silently raising the minimum. Clove is technical prior art, not a mandate to add its features. All domain rules and data integrity decisions remain in the shared core.
- The first Web version binds only local loopback addresses and exposes no LAN or public binding option. For page and API requests, accept `Host` only for `localhost` or a loopback IP at the actual listening port; reject arbitrary domains and other ports. When `Origin` is present require the same scheme, host and port as the requested service origin; reject `Origin: null` and mismatches. Requests without `Origin` may proceed subject to Host validation. Do not allow arbitrary cross-origin API access. Safely render titles and Markdown using a maintained renderer: raw HTML, scripts, unsafe links and embedded markup must not execute.
- The Web implementation includes a development/test launcher: discover the project from the current working directory by default or accept an explicit project path; fail clearly without initializing or modifying a missing or invalid project. The developer installs frontend dependencies from the lockfile and builds local assets before launch. Missing assets cause a clear startup error and preparation guidance, not automatic npm, dependency downloads, or a placeholder. Ordinary Rust compilation and basic-build tests do not run npm or require frontend assets.
- Formal `waymark serve`, the optional Web feature, embedded release assets, installation, packaging, downloads and release automation are separate distribution work. That work must reuse this Web service and frontend output rather than creating a second implementation.

### History and scope discipline

- History and diff use Git history and worktree or baseline comparisons where available. No separate application audit log, authenticated attribution, or team audit service is implied.
- Git checkout, revert, and reset are interpreted entirely from current authoritative files. Fingerprint changes invalidate old projections; no hidden restore log exists and no other file is automatically repaired. Reintroduced archived targets and old relationships are diagnosed from current content.
- The specification defines capabilities and behaviour, not delivery phases or a release schedule.
- There is no import or export capability and no cross-work-item batch surface.
- Git is optional. A probe checks both a containing repository and a `git` executable on `PATH`. If either is missing, history and diff succeed with an empty payload, an informational finding, and the usual exit code; no tracker operation is blocked or degraded.
- `GIT_UNAVAILABLE` reports a missing `git` executable and `NOT_A_GIT_REPOSITORY` reports that no repository contains the project. When both are missing, both findings are reported, each naming its cause. Both are info severity, non-fixable, and never change the exit code; `doctor` and `validate` report them without installing Git or initializing a repository.
- Both adapters expose history `{item?, path?, limit?}` and diff `{item?, commit?}` with the same parameters. History defaults `limit` to 20, where `0` means unlimited; `item` and `path` are mutually exclusive; each invalid combination is rejected by both adapters, with the CLI reporting it as `USAGE_ERROR` and MCP as `VALIDATION_ERROR`. The CLI renders human lines or the raw patch and MCP returns the shared envelope. No Git capability exists in only one adapter; Git-based stats, blame, and status are not part of v1.
- Git paths are literal project paths on every platform, never pathspec magic or wildcards. `git history --path` uses `/`, rejects backslashes, absolute and drive-relative forms and any `..` component, allows standalone `.` for the project root, and may name a path that no longer exists in the working tree. The query is bounded to the project; a Git query may not silently widen to the containing repository or to another project inside it.
- History and diff locate the working tree that actually contains the project, including a project in a repository subdirectory, a permitted project-root alias, and a linked worktree. When that scope cannot be established, the query fails explicitly instead of guessing a prefix, widening the scope, or returning an ordinary empty result. Informational empty results remain only for a confirmed unavailable Git executable or a confirmed project outside any repository.
- A working-tree diff for a specified untracked or ignored work item returns a nonempty, applicable new-file patch on every supported platform, produced from the file's actual working-tree content. On Windows this replaces any platform-specific empty result. The patch carries paths relative to the actual working tree; CLI human stdout contains the patch only, while CLI JSON and MCP carry the same content structurally.
- Untracked patch generation disables external diff and textconv so no arbitrary program can shape the output, preserves Git's path quoting, and supports a Git binary patch for content Git classifies as binary rather than substituting a `Binary files differ` notice. A non-zero `diff --no-index` exit status is never taken as proof of a valid patch. When attributes, Git version, environment, or content prevent a safe applicable patch, or when the patch cannot be carried losslessly by the text transport, the operation fails explicitly instead of fabricating one.
- A storage-root-scoped diff covers tracked files only and does not automatically include untracked work items; an empty result therefore does not establish that no untracked files exist. Root `.` may include tracked project files that are not work items, and categories outside the storage root may be omitted. CLI documentation and MCP results state that scope rather than implying complete work-item coverage.
- When the repository has no `HEAD`, a working-tree diff compares against an empty tree: an untracked item still yields a whole-new-file patch, and staged content is compared in its current working-tree state.

## Testing Decisions

- Test externally visible behaviour and stable contracts, not internal helper structure. Assert persisted files, returned results and errors, and rendered user-visible output.
- Use the shared domain library as the highest and primary seam. Exercise lifecycle and readiness, relationship validation and symmetry, ownership and advisory operator warnings, custom-field validation and query semantics, archive effects, version and migration behaviour, deterministic serialization, fingerprint conflict handling, projection completeness states, and the write-lock timeout through its public operations against temporary project data. This one seam covers every adapter, because adapters add no business rules.
- Test the CLI and MCP adapters at their own seam for argument and protocol translation, versioned envelope shape, shared error-code passthrough, warning metadata, and exit-code mapping. Do not duplicate the full domain suite in each adapter; assert that both route to the same behaviour and that neither invents a second taxonomy.
- Test the read-only Web at its public UI and API seam: combined and category-grouped hierarchy, cross-category children, category shapes and work-state colours, dependency overlays and reference nodes, detail contents, expand and collapse, browser-local persistence and project isolation, unavailable storage, responsive, empty, and unresolved states, archived exclusion, and the absence of any write path. Use automated frontend tests for tree rendering, overlay toggling, detail, and persistence, plus real HTTP tests for reads, malformed requests, rejection of writes, Host and Origin restrictions on pages and APIs, and safe rendering of hostile titles, body, and links. Test that an incomplete projection reports failure and diagnostics instead of a partial complete tree, and that the local launcher fails clearly when frontend assets are absent. Formal basic/Web feature builds and distributable-build missing-asset failure tests belong to the separate distribution work.
- Pin the error registry with the four mechanisms it requires: compile-time exhaustiveness over error variants, registry-consistency, JSON-schema enum round-trip, and a golden snapshot of each code's exit class, retryability, details keys, and location kinds. The same snapshot pins warning codes and their detail keys. Treat a registry change as a tested change.
- For concurrency and migration, assert that nothing is silently overwritten, that a stale preview cannot be applied, that dry runs write nothing, that rollback failure is reported explicitly, and that recovery diagnostics name the affected targets, rather than asserting lock or staging internals. Assert that each affected path appears in exactly one of the restored, failed, and unknown states, that an existing empty migration target is a conflict, that a target created after prevalidation is not overwritten, and that cleanup removes only directories this run created and can still identify.
- Test warning carriage behaviourally: a successful replacement whose directory synchronization fails still reports success and carries `DIRECTORY_SYNC_UNCONFIRMED` with its directory and affected paths; a platform without that capability emits no per-write warning and is documented as a limitation; a warning never changes the success status or the exit code.
- Run the suite natively on Linux, macOS, and Windows for both `x86_64-pc-windows-msvc` and `x86_64-pc-windows-gnu`; cross-compilation is never accepted as evidence for platform behaviour. A skipped case must carry a stable identifier and reason registered in a version-controlled allowlist, and an unlisted skip fails the run. At least one environment must actually execute the capability-dependent cases, and any capability no environment can exercise is a documented coverage gap rather than an implicit pass.
- A failing gate is never converted into a pass by retrying it. A nondeterministic test is made deterministic or quarantined with a tracking reference and a deadline; quarantine never covers lock safety, replacement and rollback, migration, or path validation.
- Native coverage must include portable path validation, case and Unicode collisions, symlink and reparse-point refusal, project-root aliases, linked worktrees, repositories with no `HEAD`, ignored and untracked work items, and applying generated patches in isolated clean worktrees. String-only tests never establish filesystem behaviour.
- Test the three-state recovery report and the path tagged union: a valid UTF-8 path serializes as a string and a non-representable path as a base64 object, with no lossy substitution anywhere in the response.
- Test deterministic serialization by round-tripping documents to byte-identical output, by checking that unset values are omitted rather than written as null, by checking relationship list order, and by checking exactly one trailing newline for a non-empty body.
- Test archive as a batch: target metadata preserved, `updated_at` unchanged on the target, inbound core relationships removed from active files with their `updated_at` updated, body and custom fields untouched, active-children rejection, re-archive rejection, archived-item write rejection, archived-only query range, and `show` of an archived item.
- Test migration classes at their public seam: version rejection and acceptance bounds, dry-run report contents, schema add-required-field and remove-field atomicity, storage prevalidation rejections, and the warning outside a Git repository.
- Test projection and cache rules behaviourally: a broken file still permits local reads, global graph queries fail explicitly, metadata-only fingerprint changes do not force semantic re-import, and cached data cannot serve a stale or incomplete result.
- Clove provides prior art for typed error contracts, canonical error-code mapping, versioned envelopes, diagnostics, cycle reporting, and adapter tests. Waymark's own module seams will be established when its codebase exists.

## Out of Scope

- Implementing Wayfinder-specific map, decision-ticket, frontier, assignment, or claim semantics in the core.
- Web writes, in-page refresh, and any separate Web archive browser.
- Import and export.
- A separate application audit log or team audit service.
- Physical deletion as an ordinary operation.
- Authentication or security guarantees based on the local operator/owner configuration.
- Category-specific schemas, reference semantics for custom fields, arbitrary query DSLs, nested custom data, and general workflow DSLs.
- Cross-work-item batch commands.
- Windows ARM64, Cygwin, and WSL-specific behaviour.
- Concurrent access over network filesystems, including NFS and SMB.
- Filesystems other than local NTFS on Windows; FAT, exFAT and network shares are rejected rather than degraded.
- Windows installers (MSI or MSIX), a Windows service, code signing, and auto-update.
- Claiming platform support without native CI evidence.
- Delivery phases, release sequencing, and rollout plans; detailed build, packaging, installation and distribution design is handled separately from the Web implementation.
- TUI, daemon, external tracker synchronization, notifications, and plugins.
- Anything the Waymark design map still records as undecided.

## Further Notes

This specification is derived from the Waymark design map and the Chinese CLI and MCP interface working document; both are decision history, and the interface working document is superseded once this specification is accepted. The previous functional specification and its implementation tickets are retired and must not be implemented from.

Use the project glossary terms: work item, category, owner, executor, creator, operator, parent, dependency, archive, closed, blocked, blocked by, ready, soft link, custom field, field type, stable ID.

Clove is a behavioural reference for typed errors, canonical error-code mapping, versioned envelopes, diagnostics, cycle reporting, and the frontmatter-plus-body document model — not a feature checklist.

### Resolved interface details

All earlier interface details — the format-generation field, projection synchronization, MCP transport and tool naming, human-readable CLI output and colour, MCP edit schemas, composite editing, field projection and the rank tie-break, migration envelopes, and Git parity — were resolved on 2026-11-04; the Waymark design map (`wm-y14hs95k8wct`) records those decisions. Distribution, packaging, and release automation remain outside this specification and are handled separately.
