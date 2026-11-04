---
schema: 1
id: wm-2qd0yahgpa8t
title: Define generic custom-field filtering and projection semantics
category: map
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:20Z
updated_at: 2026-11-03T23:52:22Z
closed_at: 2026-11-03T23:52:20Z
parent: wm-y14hs95k8wct
dependencies:
  - wm-wr11be4s3vpd
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
