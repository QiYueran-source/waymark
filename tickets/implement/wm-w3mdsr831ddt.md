---
schema: 1
id: wm-w3mdsr831ddt
title: Document installation, licences, changelog, and MCP clients
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-04T15:38:12Z
updated_at: 2026-11-04T15:54:29Z
closed_at: 2026-11-04T15:54:29Z
parent: wm-4125jxq7bpq8
---
## Parent

wm-4125jxq7bpq8 — Waymark distribution, packaging, and release. Read its "Documentation and changelog" decisions and the relevant user stories.

## What to build

The repository has no README, no licence files, no changelog, and no MCP client guide. Write the user-facing release documentation so a user can install, verify, upgrade, and connect Waymark without reading the source. Packaging (a sibling ticket) copies these files into the release archives.

## Acceptance criteria

- [ ] `README.md` at the repository root has an Install section per platform: archive name (`waymark-v<version>-linux-x86_64.tar.gz`, `waymark-v<version>-macos-aarch64.tar.gz`, `waymark-v<version>-macos-x86_64.tar.gz`), `.sha256` verification, extraction, and first commands (`waymark version`, `waymark init`, `waymark new`, `waymark list`).
- [ ] The README tells a macOS user the binary is unsigned, what Gatekeeper will do, and how to proceed; it is not left to discovery by failure.
- [ ] The README documents upgrading a basic install to the Web package by replacing the binary, with `.waymark/` and every work item unchanged, and documents `waymark serve` (loopback only, default port 7331, `--port`, `--open`) plus what a basic build reports instead.
- [ ] `docs/mcp-clients.md` gives working stdio configuration examples for Claude Code, DSH, and a generic JSON client, all using `waymark-mcp`.
- [ ] `CHANGELOG.md` follows Keep a Changelog and carries an `0.1.0` entry for this release.
- [ ] `LICENSE-MIT` and `LICENSE-APACHE` hold the standard licence texts matching the workspace `license = "MIT OR Apache-2.0"`.
- [ ] Every command, flag, archive name, and path named in the docs matches the implemented surface in this release; write nothing aspirational.

## Blocked by

None. The CLI surface is pinned by the specification; if an implemented command disagrees when you check, report it instead of documenting the disagreement.
