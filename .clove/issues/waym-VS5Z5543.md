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
- [Define issue lifecycle, archive/delete, and reference integrity](waym-B85RM1BH): Archive is reversible and preserves status/relations; delete is excluded from ordinary initial use and must not break references.
- [Define owner transfer, operator guardrails, and write authorization semantics](waym-TNAYVX4S): Create may assign another human owner; every later mediated write checks declared operator against owner, transfers take effect immediately, and batches are all-or-nothing; identities are not authenticated.
- [Define project-level custom field types and schema syntax](waym-0Y4VRHNA): Custom fields use basic shape types only (string, integer, boolean, homogeneous list); enum is a string convention, fields are optional by default, and custom values have no reference/workflow behavior.
- [Define generic custom-field filtering and projection semantics](waym-HDBFBM0Y): A finite generic query set supports presence, equality, typed comparisons and string matching; list membership is explicit; null is disallowed; sorting and projection have deterministic missing-value behavior, with strict type errors.
- [Define configuration and frontmatter evolution and migration policy](waym-YGVH6XQX): One command-managed `config_version` covers config and frontmatter; incompatible migrations are explicit, dry-runnable, prevalidated, backed up under `.waymark/`, and atomic; older files can be lazily upgraded on write, future-version or invalid data is rejected, serialization is deterministic, and archive data is also stored under `.waymark/`.
- [Define file concurrency and index behavior](waym-2YJB5DZ6): Work-item files are authoritative; a deterministic, Git-tracked JSON index at `.waymark/index.json` is rebuilt from files, validated for freshness, and never manually merged; writes use project locking and optimistic conflict checks, while batches stage and roll back as an application-level all-or-nothing operation.
- [Define read-only Web tree overview and navigation](waym-CMBAPDH0): Web is a strictly read-only visual overview with combined/category-grouped trees, category shape cues, work-state colors, optional dashed dependency overlays, simple details/navigation, and browser-local view-state persistence; archived items are excluded by default.
- [Define Waymark delivery scope, archiving UI, and history capabilities](waym-J12VZ4AX): No separate archive browser, no delivery-phase commitments, no import/export or separate audit log; Git provides history/diff, and the resolved tree overview is the complete Web scope.
- Stable IDs are independent of category, name, and path; storage root and category paths are configurable.
- Categories are project-defined groupings, without category-specific field schemas in the initial model.
- Parent is a single cross-category hierarchy edge; parent and hard dependencies are independent, and only dependencies block readiness. Parent lifecycle does not cascade to children.
- Core fields have stable meanings. Project custom fields are declared at project scope, uniquely named, optional by default, and do not implicitly alter core behavior. Executor is project-defined rather than a Waymark core field; generic field querying must be available.
- Creator is provenance; owner is required human accountability. Only the current owner may initiate an owner transfer, and owners must be human.
- The configured operator/owner check is a process guardrail, not authentication. Executor metadata does not grant write authorization.
- Clove's strict, predictable frontmatter is a format reference, but Clove itself has no custom-field schema/query support.

## Not yet specified

None identified. The current route is clear for producing the functional specification.

## Out of scope

- Implementing Waymark in this map.
- Wayfinder-specific map/frontier workflow semantics in the Waymark core.
- Agent claim/assignment prompting conventions; those belong to the relevant Agent skill or project workflow.
- Separate Web archive browsing, import/export, and a standalone application audit log; these are not part of this specification's scope.