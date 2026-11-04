# Waymark (Rust) — agent notes

Waymark itself: a local-first, Git-friendly work-item tracker. The functional specification and the
implementation tickets live in the tracker at `.clove/issues/` in the repository root. Read those
before changing behaviour — the specification is normative.

## Layout

- `crates/waymark-core` — every domain rule. The CLI, MCP server, and Web view are adapters and must
  not hold rules of their own.
- `crates/waymark-cli` — the `waymark` binary (transport and rendering only).
- `crates/waymark-mcp` — the `waymark-mcp` MCP server over stdio (transport and rendering only). Its
  tools map one-to-one onto core operations; it adds no rules and no second error taxonomy.
- `crates/waymark-web` — the read-only Web service (`waymark-web-dev` launcher, Axum + Tokio) and
  `crates/waymark-web/ui`, the SvelteKit/Svelte/TypeScript frontend it serves. There is no write path.
- `waymark_core::view` — the shared JSON renderers every adapter calls. Do not copy a renderer into an
  adapter; add it here and let the CLI, MCP, and Web share it.
- `docs/implementation-decisions.md` — the record of choices the specification leaves open.

## Quality gate

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Frontend (only when changing `crates/waymark-web/ui`); `cargo` never needs npm:

```sh
cd crates/waymark-web/ui
npm ci && npm run build && npm test && npm run check
```

Set `CARGO_HOME` and `npm_config_cache` to writable paths in this environment; the defaults are not
writable.

## Conventions

- Work-item documents are the sole authority; the in-memory projection is derived and disposable.
- Serialization is canonical and byte-stable: fixed key order, unset optional values omitted,
  relationship lists sorted and de-duplicated, non-empty bodies ending in exactly one newline.
- Every mediated write takes the one project lock on `.waymark/write.lock` and uses fingerprint
  conflict detection; never silently overwrite a concurrent edit.
- Errors are typed and carry a registered code; exit classes come from one mapping table.
- `docs/implementation-decisions.md` records choices the specification leaves open. Add to it instead
  of inventing a second opinion in code.
