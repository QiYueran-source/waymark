---
schema: 1
id: waym-DRMHS4C3
title: Read-only Web tree overview
status: open
type: feature
priority: 2
created: 2026-11-03T05:43:42.082Z
updated: 2026-11-03T05:43:42.082Z
parent: waym-WMSPEC01
labels: [ready-for-agent]
deps: [waym-Y1ZTWK25]
relates: []
duplicates: []
supersedes: []
---
## Parent

waym-WMSPEC01 — Waymark functional specification.

## What to build

Implement the functional specification's strictly read-only Web overview, using Axum and Tokio for the backend and SvelteKit, Svelte, TypeScript, and Vite for the frontend. Reuse the shared core for all domain behavior; do not implement HTTP by hand. Validate selected Rust dependency versions and the resolved graph against the supported Rust version; report incompatibilities rather than silently raising it.

Deliver a development/test launcher and locally built frontend so the real UI and API can be exercised independently in a browser. T12 owns Web dependencies and implementation, not formal `waymark serve` integration, optional CLI feature gating, release-asset embedding, installation, packaging, downloads, or release automation. Later distribution work must reuse this Web server and frontend output.

## Acceptance criteria

- [ ] the tree renders non-archived items top-down, combined by default and grouped by category, with cross-category children still under their real parent
- [ ] categories are distinguished by stable shapes, and work state by colour plus a text marker for blocked
- [ ] dependency overlays are optional dashed edges with reference nodes for cross-category targets, toggled per view
- [ ] selecting an item opens a read-only detail with body, category, actual status, blocked reason, owner, existing custom fields, and relationships; custom-field names acquire no special semantics, and navigation preserves the tree position
- [ ] selected view, per-view expansion and dependency visibility, and scroll position persist only in the browser, isolated per project within an origin using an identifier derived from the normalized project-root path; different origins or a moved project may start from defaults, and neither preferences nor identifier are written to the project or uploaded
- [ ] if browser storage is unavailable or saving fails, the UI remains usable during the session and gives a concise persistence warning
- [ ] archived items are excluded, and empty and unresolved-relations states are legible; known missing parents are shown as unresolved, while incomplete projections or damaged files that prevent a complete tree yield an explicit read failure and available diagnostics, never a partial tree presented as complete
- [ ] the local development/test launcher discovers the project from the working directory by default or accepts an explicit project path; missing or invalid projects fail clearly without initialization or modification
- [ ] developers install frontend dependencies from the lockfile and build local assets before starting the launcher; missing assets produce a clear startup error with guidance, without invoking npm, downloading dependencies, or serving a placeholder
- [ ] ordinary Rust compilation and basic-build tests do not automatically run npm or require frontend assets
- [ ] the service binds only loopback and has no LAN/public binding option; pages and APIs validate Host as localhost or a loopback IP on the listening port, and reject arbitrary domains and wrong ports
- [ ] for pages and APIs, Origin when present must match the service scheme, host, and port; reject `Origin: null` and other mismatches; requests without Origin may proceed after Host validation, and arbitrary cross-origin API access is not enabled
- [ ] titles and Markdown bodies render safely using a maintained Markdown library; raw HTML, scripts, unsafe links and embedded markup cannot execute
- [ ] no request from the Web view can modify project data; no search/filter replacement, in-page refresh, or archive browser is added
- [ ] automated frontend tests cover tree rendering, dependency overlay toggling, detail and browser-state persistence and isolation; functional, responsive and accessibility acceptance in the specification remains covered
- [ ] real HTTP tests cover reads, malformed requests, absence of writes, and Host/Origin protection on pages and APIs; security tests cover hostile titles, bodies and links; tests cover incomplete-tree errors and missing local assets at launch

Separate basic/Web feature builds and distributable-build missing-asset checks are acceptance for later distribution work, not T12.