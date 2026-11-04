# Waymark Web UI (T12 phase 2)

A read-only SvelteKit + Svelte 5 + TypeScript single-page overview of a Waymark
project. It consumes the existing loopback JSON API in `crates/waymark-web` and
has no write path: no forms, no non-GET requests, no in-page refresh, and no
archive browser.

## Install, build, and test

Run from this directory (`crates/waymark-web/ui`):

```sh
npm ci          # install exactly what package-lock.json pins
npm run build   # static SPA output in ui/build
npm test        # vitest + @testing-library/svelte + jsdom
npm run check   # svelte-check type/diagnostic pass
```

`node_modules/`, `build/`, and `.svelte-kit/` are local artifacts and are
ignored; `package-lock.json` is committed.

`npm run build` emits `ui/build/index.html` and hashed files under
`ui/build/_app/...`. The service serves the shell at `/` and maps `/_app/<path>`
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

Four preferences are stored, in `localStorage`, namespaced by a `cyrb53` hash of
the normalized project root reported by `GET /api/health` (`data.project.root`):
the selected view, per-view expansion, per-view dependency-overlay visibility,
and per-view scroll position. The keys are
`waymark.web.v1:<hash>:<preference>[:<view>]`, so two projects on one origin never
read each other's state and each view keeps its own expansion and overlay choice.
Nothing is written into the project or sent to the service. Unavailable or
failing storage degrades to in-memory state with a concise notice.

One more preference is stored per project rather than per view, at
`waymark.web.v1:<hash>:categories`: the category keys the reader has in scope in
the by-category tree. The value is a JSON array of keys, and its absence means
"every configured category"; an empty array means the reader cleared them all, so
the two cases stay distinguishable. Keys for categories this project no longer
configures are ignored when the scope meets the payload's `categories`, which
also lists configured categories that have no active work yet.

## Layout

- `src/lib/components/` — `Explorer` (data loading and state), `CategoryFilter`,
  `TreeView`, `NodeRow`, `DetailPanel`, `UnresolvedList`, `ReadFailure`,
  `ReferenceList`, `Shape`.
- `src/lib/api.ts` — GET-only envelope client and failure types.
- `src/lib/storage.ts` — project-scoped browser preferences.
- `src/lib/tree.ts` — pure rows, dependency edges, grouped reference nodes, and
  the category scope and filtering used by the by-category tree.
- `src/app.css` — the whole visual system in plain CSS (tokens, tree, detail,
  states, responsive); there is no UI framework and no icon package.
- `tests/` — vitest suites for combined/grouped rendering, overlay toggling and
  reference anchoring, detail contents and the HTML-injection boundary,
  expand/collapse, persistence and per-project isolation, unavailable storage,
  category selection and filtering, empty, unresolved, and read-failure states.

## Visual direction

"Field index": a surveyor's field sheet for a work tree. Cool paper ground,
instrument ink, one deep-teal accent for interaction, category identity as a
stable shape plus name, and work state as a narrow colour stripe plus a state
word (with the blocked reason in prose). No images, gradients, shadows, or
external fonts; the screen is a tool, not a landing page.
