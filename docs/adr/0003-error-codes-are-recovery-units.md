# Error codes are recovery units, not error variants

The CLI, the MCP server, and diagnostics share one flat registry of `SCREAMING_SNAKE` codes. A code exists only when the recovery action differs; when only the offending value differs, the difference goes into typed details. `VALIDATION_ERROR`, `PARSE_ERROR`, `CYCLE_DETECTED`, and `INVALID_RELATIONSHIP` each deliberately cover a family of conditions. One code per condition would produce a table that neither adapter can keep honest as it evolves, while a single generic error would force callers to parse message text to decide what to do.

## Considered Options

- One code per condition: rejected because the table would grow past a hundred entries and drift between adapters.
- The numeric exit class as the contract: rejected because it is too coarse to select a recovery action.
- Stable message prefixes: rejected because callers would end up parsing prose.

## Consequences

- Each code records its exit class, retryability, typed details, and location kinds, so neither adapter classifies by prefix or name.
- The registry is pinned by compile-time exhaustiveness, a consistency test, a JSON-schema round-trip, and a golden snapshot, so adding a code is a deliberate, reviewed change.
