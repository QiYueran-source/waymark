# Waymark (Rust) — agent notes

Waymark itself: a local-first, Git-friendly work-item tracker. The functional specification and the
implementation tickets live in the tracker at `.clove/issues/` in the repository root. Read those
before changing behaviour — the specification is normative.

## Layout

- `crates/waymark-core` — every domain rule. The CLI, MCP server, and Web view are adapters and must
  not hold rules of their own.
- `crates/waymark-cli` — the `waymark` binary (transport and rendering only).

## Quality gate

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Conventions

- Work-item documents are the sole authority; the in-memory projection is derived and disposable.
- Serialization is canonical and byte-stable: fixed key order, unset optional values omitted,
  relationship lists sorted and de-duplicated, non-empty bodies ending in exactly one newline.
- Every mediated write takes the one project lock on `.waymark/write.lock` and uses fingerprint
  conflict detection; never silently overwrite a concurrent edit.
- Errors are typed and carry a registered code; exit classes come from one mapping table.
- `docs/implementation-decisions.md` records choices the specification leaves open. Add to it instead
  of inventing a second opinion in code.
