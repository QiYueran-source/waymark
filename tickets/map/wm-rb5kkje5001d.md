---
schema: 1
id: wm-rb5kkje5001d
title: Investigate Waymark MCP and Web runtime prerequisites
category: map
status: closed
owner: frank
created_at: 2026-11-04T02:01:58Z
updated_at: 2026-11-04T11:55:21Z
closed_at: 2026-11-04T11:55:21Z
parent: wm-ztbcsgpkq248
---
## Question

What artifacts, commands, project discovery behavior, build dependencies, and existing tests are needed for generic MCP stdio access and for the Web runtime used by the DSH integration? Identify missing or unfinished distribution pieces without silently changing the established packaging design.

## Research context

Local research branch: `research/waymark-runtime`. Findings are being prepared at `docs/research/waymark-runtime.md` on that branch; they are not merged or pushed.

## Resolution

Verified against the checked-in implementation on 2026-11-04 and re-checked against integration HEAD `6a18981`; the changes since the research baseline `5a080b4` are Web UI, tests and documentation, so the runtime facts below still hold.

- **MCP:** a separate `waymark-mcp` binary using `rmcp 0.8` over stdio, tools only (no resources or prompts), with read and write tools. Form: `waymark-mcp [--project DIR]`; current directory by default; `--project` is a discovery start directory, not necessarily the project root; stdout is reserved for the protocol. Discovery walks up to the nearest `.waymark/config.toml`, and MCP rediscovers per applicable call. Success uses the shared `{v, ok, data, _meta}` envelope; domain failures are structured tool results. Writes are single-target, with no batch API.
- **Web:** the only entry point is the development/test launcher `waymark-web-dev [--project PATH] [--port PORT] [--assets DIR]`. There is no `waymark serve`, no web feature in `waymark-cli`, and no distributable Web-enabled binary. Default assets resolve through compile-time `CARGO_MANIFEST_DIR`; `ensure_assets` checks only `index.html`; assets are external, not embedded; missing assets fail before binding. The listener binds `127.0.0.1` only, accepts GET/HEAD only, enforces the Host/Origin guard, emits no CORS headers, has no authentication, and sets no CSP or `X-Frame-Options` in the inspected response code.
- **Build:** the workspace declares `rust-version = 1.85`. An earlier `cargo metadata --locked` audit found no declared package MSRV above 1.85, but this checkout was not proven built or tested on 1.85. The frontend needs Node.js/npm and the lockfile.
- **Missing for distribution:** `waymark serve`, the basic/Web build feature, embedded assets, separately named prebuilt packages, release automation, and frontend asset provenance/version verification. These are the established packaging design's open pieces; this ticket does not change that design.

Findings: `docs/research/waymark-runtime.md` on local branch `research/waymark-runtime` (commit `9da7e14`).
