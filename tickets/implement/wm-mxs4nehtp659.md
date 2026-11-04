---
schema: 1
id: wm-mxs4nehtp659
title: Gate pull requests with CI
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-04T15:38:17Z
updated_at: 2026-11-04T16:41:05Z
closed_at: 2026-11-04T16:41:05Z
parent: wm-4125jxq7bpq8
dependencies:
  - wm-vkasp2ade4j7
---
## Parent

wm-4125jxq7bpq8 — Waymark distribution, packaging, and release. Read its "Release gates" decisions and the "Testing Decisions" section.

## What to build

The repository has no CI. Add the first pull-request pipeline so formatting, linting, tests, the frontend build, the build-variant split, and a real Rust 1.85 build gate every change.

## Acceptance criteria

- [ ] `.github/workflows/ci.yml` runs on pull requests and pushes to `master`, with separate jobs for: the frontend (`npm ci`, `npm run check`, `npm run build`, manifest generation), Rust on `ubuntu-latest` and `macos-latest` (`cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features`), and MSRV on Rust 1.85 (`cargo metadata --locked` plus a workspace build).
- [ ] The Rust jobs consume the frontend job's artifact, so the frontend is built once and embedding has its assets; a missing or failed frontend build fails the pipeline instead of being skipped.
- [ ] A step asserts the basic build's dependency graph excludes `waymark-web` (`cargo tree -p waymark-cli -e features`), and that the basic `serve` reports `WEB_NOT_INCLUDED` while a Web build's `version` reports `web`.
- [ ] No job or step is `continue-on-error`; any red gate fails the pull request.
- [ ] The workflow YAML parses, and the local equivalent of every Linux command is documented in a new `docs/development.md` (do not edit the README; a sibling ticket owns it).
- [ ] Locally: the workflow file parses as YAML, and every Linux command it runs was executed successfully in the ticket worktree. macOS and 1.85 legs are CI-only; say so explicitly in the report.

## Blocked by

"Add `waymark serve` and `waymark version` build variants" — the `web` feature, the variant assertion, and the two binaries' behaviours must exist first.
