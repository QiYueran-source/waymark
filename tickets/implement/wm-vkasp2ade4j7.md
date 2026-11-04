---
schema: 1
id: wm-vkasp2ade4j7
title: Add waymark serve and waymark version build variants
category: implement
status: open
owner: frank
creator: frank
created_at: 2026-11-04T15:38:14Z
updated_at: 2026-11-04T15:38:14Z
parent: wm-4125jxq7bpq8
dependencies:
  - wm-jd7fyh213aa1
---
## Parent

wm-4125jxq7bpq8 — Waymark distribution, packaging, and release. Read its "Build variant" and "`waymark serve`" and "Version reporting" decisions.

## What to build

`waymark` gains the release-facing surface: `waymark serve` (the real Web server in a Web build, an explicit typed refusal in the basic build) and `waymark version` reporting capabilities and the embedded asset manifest. An off-by-default `web` feature on `waymark-cli` selects the variant, so a basic build never compiles `waymark-web`.

## Acceptance criteria

- [ ] `waymark-cli` declares `[features] web = ["dep:waymark-web", "waymark-web/embed"]` and an optional `waymark-web` dependency; the default build is basic; there is no `full` alias; `cargo tree -p waymark-cli -e features` for the default build does not list `waymark-web`.
- [ ] `serve [--port PORT] [--open]` exists in both builds, honours the global `--json`, `--color`, `--project`, and `--lock-timeout-ms` flags, discovers the project exactly like the other commands, and fails before binding on a missing or invalid project without creating or modifying anything.
- [ ] Basic build: `serve` fails with registered `WEB_NOT_INCLUDED` (validation exit class) whose message names the Web package; it never downloads, installs, or shells out to anything.
- [ ] Web build: binds `127.0.0.1` only on `waymark_web::DEFAULT_PORT` (7331) by default, `--port 0` asks the operating system for a free port, and the embedded tree from the embedding ticket is served.
- [ ] A busy port fails with `PORT_IN_USE` naming the port and suggesting `--port 0`; there is no silent fallback to another port.
- [ ] Human mode prints `serving http://127.0.0.1:<port>` on stderr before blocking; `--json` emits `{"url": "http://127.0.0.1:<port>"}` in the versioned envelope, then blocks serving.
- [ ] `--open` runs `xdg-open` on Linux or `open` on macOS; a launch failure is a warning, never a serve failure; no browser is opened without the flag.
- [ ] `waymark version` prints `waymark <version> (<capabilities>)` listing `cli, mcp` and `web` only in the Web build; `waymark version --json` returns `{version, capabilities, assets}` with `assets` absent in the basic build and the embedded manifest in the Web build.
- [ ] `WEB_NOT_INCLUDED` and `PORT_IN_USE` join the frozen core error registry with exit class, retryability, details keys, location kinds, and finding severity; the golden snapshot at `crates/waymark-core/tests/golden/error_registry.json` and the registry-consistency tests change in the same commit. Do not edit archived specification files.
- [ ] Tests cover: basic `serve` failure and its exit class, Web `serve` over real HTTP with a free port, a busy port, the readiness line, JSON envelope ordering, and both `version` shapes.

## Blocked by

"Embed the Web frontend and its provenance manifest" — the embedded manifest and embedded asset source must exist first.
