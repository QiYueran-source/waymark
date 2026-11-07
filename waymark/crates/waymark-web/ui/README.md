# Waymark Web UI (T12 phase 2)

A read-only SvelteKit + Svelte 5 + TypeScript single-page overview of a Waymark
project. It consumes the existing loopback JSON API in `crates/waymark-web` and
has no write path: no forms, no non-GET requests, no in-page refresh, and no
archive browser.

## Install, build, and test

Run from this directory (`crates/waymark-web/ui`):

```sh
npm ci          # install exactly what package-lock.json pins
npm run build   # static SPA output in ui/build, plus ui/build/build-id.txt
npm test        # vitest + @testing-library/svelte + jsdom
npm run check   # svelte-check type/diagnostic pass
```

`node_modules/`, `build/`, `.svelte-kit/`, and `assets-manifest.json` are local
artifacts and are ignored; `package-lock.json` is committed.

`npm run build` emits `ui/build/index.html`, hashed files under
`ui/build/_app/...`, and `ui/build/build-id.txt` (a random id the Vite plugin in
`vite.config.ts` writes after the adapter finishes). A Web release then records
the tree in `ui/assets-manifest.json` with the Rust generator:

```sh
# from the repository root
cargo run -p waymark-web --bin waymark-web-manifest
```

The generator walks `ui/build`, hashes every file, and fails if `index.html` or
`build-id.txt` is missing. `cargo build -p waymark-web --features embed` embeds
the tree only when that manifest is present and matches the tree exactly; the
development launcher below does not need it. The service serves the shell at `/` and maps `/_app/<path>`
to `<assets>/_app/<path>` (SvelteKit's default asset prefix, which is why
`svelte.config.js` leaves `kit.paths.base` and `kit.paths.assets` unset and only
sets `kit.paths.relative = false`). The backend also serves the same files under
`/assets/<path>`.

Then, from the repository root, launch the development service:

```sh
cargo run -p waymark-web --bin waymark-web-dev -- --project /path/to/project
# port 0 asks the OS for a free port; --assets points somewhere other than ui/build
```

The launcher never runs npm and never downloads anything. Without a build it
exits with a message pointing back at `npm ci` / `npm run build`. Ordinary
`cargo build` and `cargo test` never need npm or `ui/build`; the built-asset
integration test in `crates/waymark-web/tests/frontend.rs` skips with a message
when `ui/build/index.html` is absent.

## Browser state

Nine preferences are stored, in `localStorage`, namespaced by a `cyrb53` hash of
the normalized project root reported by `GET /api/health` (`data.project.root`).
Five belong to the project: the selected presentation (`tree`/`graph`), whether
the graph is read as its structured text alternative, the category scope, the
graph's collapsed node ids, and the graph viewport. Four belong to the selected
data view: `view`, expansion, dependency-overlay visibility, and scroll position.
The keys are `waymark.web.v1:<hash>:<preference>[:<view>]`, so two projects on one
origin never read each other's state and each data view keeps its own expansion,
overlay, and scroll choice. Root normalization trims, unifies separators, folds
`.` and `..`, and strips trailing slashes, so two spellings of one project share a
namespace. Nothing is written into the project or sent to the service.
Unavailable or failing storage degrades to in-memory state with a concise notice.

The category scope at `waymark.web.v1:<hash>:categories` is a JSON array of keys;
its absence means "every configured category" and an empty array means the reader
cleared them all, so the two cases stay distinguishable. Keys for categories this
project no longer configures are ignored when the scope meets the payload's
`categories`, which also lists configured categories that have no active work
yet.

Filtering never rewrites parentage. When a work item in scope has a parent in a
category the reader turned off, that ancestor — and every unselected level above
it — stays in the tree and is drawn as a context row: a hatched ground, a dashed
state stripe, and the visible text `Context — outside the selection`, which also
enters the row's accessible name after its category and state (for example
`Login fails — Bugs, Blocked (context, outside the selection)`). Context rows are
presentation only: they are excluded from the summary counts, never enter the
unresolved-relations area, and they keep only the chain down to work in scope —
an out-of-scope sibling of the chain is not dragged in.

## Layout

- `src/lib/components/` — `Explorer` (data loading and state), `CategoryFilter`,
  `TreeView`, `NodeRow`, `DetailPanel`, `UnresolvedList`, `ReadFailure`,
  `ReferenceList`, `Shape`, and the graph layer: `GraphView` (the only importer
  of the drawing library, dagre, and the library stylesheet), `GraphBridge` (its
  `useSvelteFlow` bridge), `GraphItemNode` (the library's node slot), `GraphNode`
  (one card), `GraphDependencyRoute` (the registered edge type that paints a
  routed dependency; it imports `BaseEdge` and nothing else from the library),
  and `GraphText` (the structured text alternative).
- `src/lib/api.ts` — GET-only envelope client and failure types.
- `src/lib/storage.ts` — project-scoped browser preferences.
- `src/lib/tree.ts` — pure rows, dependency edges, grouped reference nodes, and
  the category scope and filtering used by the by-category tree, including the
  context rows kept to preserve parentage.
- `src/lib/graph.ts` — the pure graph model: nodes, parent-child and optional
  dependency edges, collapse, fitted/contained viewports, accessible names, and
  the structured-outline data. No library, no Svelte.
- `src/lib/keys.ts` — the shared Enter/Space activation rule for graph titles.
- `src/app.css` — the whole visual system in plain CSS (tokens, tree, graph,
  detail, states, responsive); there is no UI framework and no icon package.
- `static/brand/` — the Waymark logo artwork. `waymark-logo-1.webp` and
  `waymark-logo-2.webp` are the two supplied variants (trimmed to their
  artwork), and `waymark-logo-2-mark.webp` is the cropped mark the header uses.
  They are served by the backend under `/assets/brand/...`.
- `tests/` — vitest suites for combined/grouped rendering, overlay toggling and
  reference anchoring, detail contents and the HTML-injection boundary,
  expand/collapse, persistence and per-project isolation, unavailable storage,
  category selection and filtering, empty, unresolved, and read-failure states,
  the pure graph model (`graph.test.ts`), and the graph's rendering, dependency
  overlay, filtering, navigation and folding, detail reuse, structured text
  alternative, and keyboard surface (`graph-view`, `graph-dependency`,
  `graph-filter`, `graph-navigation`, `graph-detail`, `graph-text`).

## Visual direction

"Field index": a surveyor's field sheet for a work tree. Cool paper ground,
instrument ink, one deep-teal accent for interaction, category identity as a
stable shape plus name, and work state as a narrow colour stripe plus a state
word (with the blocked reason in prose). No gradients, shadows, or external
fonts, and the only image is the Waymark mark in the header's top-left corner;
the screen is a tool, not a landing page.
