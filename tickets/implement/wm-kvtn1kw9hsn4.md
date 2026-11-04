---
schema: 1
id: wm-kvtn1kw9hsn4
title: Publish verified release artifacts from a tag
category: implement
status: open
owner: frank
creator: frank
created_at: 2026-11-04T15:38:20Z
updated_at: 2026-11-04T15:38:20Z
parent: wm-4125jxq7bpq8
dependencies:
  - wm-vkasp2ade4j7
  - wm-w3mdsr831ddt
---
## Parent

wm-4125jxq7bpq8 — Waymark distribution, packaging, and release. Read its "Release pipeline" and "Release gates" decisions, plus "Testing Decisions".

## What to build

A tag-triggered GitHub Actions pipeline builds both variants for Linux and macOS from one frontend build, runs the release gates, packages and verifies the archives, and attaches everything to a **draft** GitHub Release. The same artefact checks can be run locally against locally built artifacts.

## Acceptance criteria

- [ ] `.github/workflows/release.yml` triggers on `v*` tags with `contents: write`; a `version-gate` job fails unless the tag equals `[workspace.package] version`, before any build runs.
- [ ] A `web-dist` job installs from the frontend lockfile (`npm ci`), runs `npm run check` and `npm run build`, generates the manifest, uploads the tree, and packs `waymark-web-dist-v<version>.tar.gz` + `.sha256` for the source tarball.
- [ ] A build matrix covers `ubuntu-latest`/`x86_64-unknown-linux-gnu`, `macos-latest`/`aarch64-apple-darwin`, and `macos-15-intel`/`x86_64-apple-darwin`; each builds the basic and Web variants with `WAYMARK_SKIP_UI_BUILD=1`, `WAYMARK_RELEASE=1`, and `WAYMARK_GIT_SHA` set to the tagged commit.
- [ ] Packaging produces `waymark-v<version>-<os>-<arch>.tar.gz` and `waymark-web-v<version>-<os>-<arch>.tar.gz`, each holding `waymark`, `waymark-mcp`, `README.md`, `LICENSE-MIT`, `LICENSE-APACHE`, and `THIRD_PARTY_LICENSES.txt` (cargo-about), with a `.sha256` sibling. The Web package keeps the `waymark` binary name.
- [ ] The source artifact is `git archive` of the tag plus the built frontend tree, packed as `waymark-src-v<version>.tar.gz` + `.sha256`.
- [ ] Each build job runs the Web provenance check on the runner before uploading: manifest `version` equals the tag, `git_sha` equals the tagged commit, the `index.html` hash matches the manifest, and a freshly started packaged `serve --port 0` returns an `index.html` whose hash matches. A mismatch fails with `ASSETS_VERSION_MISMATCH`.
- [ ] A `release` job downloads every artifact and creates a **draft** Release with `generate_release_notes: true`, attaching both variants for every platform, all checksums, and the source tarball; nothing publishes automatically and no `cargo publish` step exists anywhere.
- [ ] `scripts/release/check-artifacts.sh` verifies checksums, archive contents, the binaries' reported version and capabilities, and the Web provenance check against downloaded draft artifacts; CI runs it on `ubuntu-latest`, and it runs locally against a locally built Linux archive.
- [ ] `docs/release-checklist.md` is a human checklist for reviewing the draft, reconciling `CHANGELOG.md` with the generated notes, running the packaged smoke checks, and publishing.
- [ ] The workflow YAML parses, and the Linux packaging plus `check-artifacts.sh` were executed successfully in the ticket worktree. macOS legs are CI-only; say so explicitly in the report.

## Blocked by

- "Add `waymark serve` and `waymark version` build variants" — packaging verifies `serve` and `version`.
- "Document installation, licences, changelog, and MCP clients" — the archives copy `README.md`, `LICENSE-MIT`, and `LICENSE-APACHE`.
