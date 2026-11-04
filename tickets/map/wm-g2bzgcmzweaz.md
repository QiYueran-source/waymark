---
schema: 1
id: wm-g2bzgcmzweaz
title: Waymark build, packaging, and distribution
category: map
status: open
owner: frank
created_at: 2026-11-04T13:04:49Z
updated_at: 2026-11-04T13:04:49Z
---
## Goal

Define how Waymark is built, packaged, and released: the basic versus Web-enabled variants, the formal `waymark serve` entry point, embedded frontend assets, artifact names, version reporting, and release automation. This map holds the decisions; its specification and implementation tickets are cut after the decisions settle.

## Context

The functional specification and the Web implementation deliberately stop at a development/test launcher. Formal `waymark serve`, the optional Web feature, embedded release assets, installation, packaging, downloads, and release automation are separate work that must reuse the existing server and frontend build output rather than building a second implementation.

## Decisions so far

- The basic distribution includes CLI, the shared core, and MCP; Web is optional. A build-time feature selects Web: the default build is the basic version, while a Web-enabled build includes Web. Adding Web to an installed basic executable requires rebuilding or installing a Web-enabled replacement, not dynamically attaching a module.
- The basic CLI retains `waymark serve` but reports clearly that Web is not included and directs the user to install a Web-enabled build; it never downloads or installs anything automatically.
- A Web-enabled build must obtain complete frontend assets. Missing assets or a frontend build failure must produce a clear build failure, rather than silently shipping a placeholder or stale frontend as a successful Web build.
- A prebuilt Web-enabled binary includes its frontend assets; an end user running it does not need Rust, Node.js, or npm. To build the frontend from source, the developer needs Node.js and npm unless verified prebuilt assets are supplied instead.
- Formal releases provide separately named prebuilt basic and Web-enabled binary packages. Both identify their version and included capabilities; the basic package includes CLI and MCP without Web, and the Web package includes those capabilities plus Web.
- Building Web from a Git checkout uses Node.js/npm and the frontend lockfile to install dependencies and build assets; failure fails the Web build. The published Web source package includes prebuilt frontend assets, avoiding a GitHub Release download at build time. These assets must correspond to the source version and release checks must validate that correspondence; the exact verification mechanism is open. Both paths embed assets in the resulting binary; Web runtime requires no Node.js/npm.
- The first Web version listens only on local loopback addresses and offers no LAN or public binding; Host/Origin rules, safe Markdown rendering, and read-only behaviour are already settled in the functional specification and Web specification.
- T12 supplies only a development/test launcher. Formal `waymark serve`, CLI feature gating, and embedding release assets belong to this distribution work and reuse T12's server and frontend build output.

## Not yet specified

- The exact artifact names, package layout, and install locations for the basic and Web packages.
- The exact Cargo feature names and how the default build stays basic without pulling frontend assets.
- The `waymark serve` CLI surface: project discovery or `--project`, port binding and default port, whether it opens a browser, and its error and readiness reporting.
- How a basic build reports Web absence from `serve`, and whether the command exists in both builds with different behaviour or only in the Web build.
- The frontend-asset provenance and verification mechanism: how a release proves embedded assets correspond to the source version, and what a mismatch reports.
- The real Rust 1.85 build gate: every locked dependency declares at or below 1.85.0, but a real 1.85 build has not run in this environment and must be a release check or CI job.
- Release automation: version stamping, checksums or signatures, changelog generation, and how basic and Web artifacts are published together.
- Installation and upgrade guidance for both packages, including how a basic user moves to Web without losing project data.
- Whether MCP client configuration guidance or examples ship with the packages.

## Out of scope

- Web UI behaviour, MCP business behaviour, and any new product feature.
- Building a second Web server or frontend.
- LAN or public network binding.
- Windows support and non-loopback deployment.

## Next steps

Settle the frontier above by grilling and record each decision here; then write the distribution specification and cut implementation tickets from it.
