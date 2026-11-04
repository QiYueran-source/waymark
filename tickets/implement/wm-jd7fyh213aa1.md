---
schema: 1
id: wm-jd7fyh213aa1
title: Embed the Web frontend and its provenance manifest
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-04T15:38:10Z
updated_at: 2026-11-04T16:02:31Z
closed_at: 2026-11-04T16:02:31Z
parent: wm-4125jxq7bpq8
---
## Parent

wm-4125jxq7bpq8 — Waymark distribution, packaging, and release. Read its "Frontend assets and provenance" decisions, "Testing Decisions", and the design map `tickets/map/wm-g2bzgcmzweaz.md`.

## What to build

`waymark-web` must be able to serve a release build whose frontend is compiled into the binary, with a generated `assets-manifest.json` that proves which tree was embedded and from which commit. Today the server only reads a directory from disk (`crates/waymark-web/src/assets.rs`, `server.rs`, `default_assets_dir`), and the frontend build in `crates/waymark-web/ui/build` has no manifest.

## Acceptance criteria

- [ ] Embedding is opt-in at build time (`waymark-web` feature `embed`, off by default). The default workspace build still compiles with no frontend on disk and no npm, and the directory-serving `waymark-web-dev` launcher keeps working.
- [ ] With embedding enabled, a missing `index.html` or a missing `assets-manifest.json` fails the build with instructions naming the frontend build step. No placeholder, stale tree, or silently empty asset set is embedded.
- [ ] A manifest generator writes `assets-manifest.json` beside `ui/build`: `{version, git_sha, build_id, files: [{path, sha256, size}]}` with sorted relative paths and per-file SHA-256; `version` is the workspace package version, `git_sha` comes from `WAYMARK_GIT_SHA` (absent locally), `build_id` is emitted by the frontend build. It fails when `index.html` is missing, and its file-walk/hash logic is a pure function with unit tests.
- [ ] `WAYMARK_SKIP_UI_BUILD=1` is accepted only to reuse a tree built earlier in the same workflow; it never removes the asset or manifest requirement. A release build (`WAYMARK_RELEASE=1`) without `WAYMARK_GIT_SHA` fails before embedding.
- [ ] Embedded assets are gzipped at build time and served with `Content-Encoding: gzip` and the existing content-type mapping; `GET /` serves `index.html` and `GET /_app/...` resolves from the embedded tree.
- [ ] The crate exposes the embedded manifest to adapters, and a test proves the served `index.html` bytes hash to the manifest's `index.html` entry.
- [ ] Over real HTTP with a `--port 0` equivalent: `GET /` is 200 with the shell, a hashed `_app` asset resolves, a non-GET method is 405, and a foreign `Host` is rejected by the existing guard.
- [ ] `docs/implementation-decisions.md` records the choices the specification leaves open (feature name, where the manifest lives, how the build id is derived, gzip timing).

## Blocked by

None.
