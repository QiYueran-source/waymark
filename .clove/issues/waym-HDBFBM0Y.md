---
schema: 1
id: waym-HDBFBM0Y
title: Define generic custom-field filtering and projection semantics
status: closed
type: feature
priority: 2
created: 2026-11-02T06:14:40.945Z
updated: 2026-11-02T07:36:30.101Z
closed: 2026-11-02T07:36:30.101Z
parent: waym-VS5Z5543
labels: [wayfinder:grilling]
deps: [waym-0Y4VRHNA]
relates: []
duplicates: []
supersedes: []
---
## Resolution

- Support a finite, generic operation set rather than arbitrary expressions or field-specific predicates.
- Custom-field filters include presence/absence (`exists`/`missing`) and equality/inequality. Strings and integers additionally support ordering comparisons. Strings may be queried with case-sensitive prefix and substring operations. Enum conventions use string equality; they do not add runtime enum behavior.
- Lists compare as whole values for equality/inequality. A separate `contains` operation tests equality membership of an element. Do not support ranges over list elements, nested paths, or nested query semantics.
- `null` is not a valid custom-field value. An unset field is distinct and matches only `missing`; it does not match value comparisons or sort as a value.
- Sort only strings and integers. Booleans and lists are not sortable. Missing values sort after set values in both ascending and descending order; equal values are tie-broken by stable ID.
- Projection supports named custom fields and all-set-fields mode. Unset fields are omitted, not emitted as `null`. Unknown field names are structured errors.
- Query operands must conform to declared field types; no implicit conversions. If persisted data violates its schema, return a locatable data error rather than silently omitting the record.

## Question

Which generic query, filter, sort, and output-projection operations should work over schema-declared custom fields? Define predictable semantics for scalar, enum, list, and reference fields without a field-specific executor command or an unbounded query DSL.