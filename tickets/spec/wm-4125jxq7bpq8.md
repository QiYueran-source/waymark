---
schema: 1
id: wm-4125jxq7bpq8
title: Waymark distribution, packaging, and release
category: spec
status: open
owner: frank
created_at: 2026-11-04T15:31:41Z
updated_at: 2026-11-04T15:31:41Z
---
# Waymark distribution, packaging, and release

## Problem Statement

Waymark today only runs from a source checkout. The Web view is served by the development launcher `waymark-web-dev`, which reads a frontend directory from the filesystem; there is no `waymark serve`, `waymark-cli` does not even depend on the Web crate, and the repository has no CI pipeline and no release process. A user who wants to track work must install a Rust toolchain, and for the Web view also Node.js and npm. A maintainer cannot show that a published archive was built from the tagged source, cannot tell a basic install from a Web-enabled one, and has no gate that stops a broken commit from being released.

## Solution

Ship two separately named, prebuilt variants from one tag-triggered GitHub Actions pipeline.

The **basic** package contains the `waymark` CLI and `waymark-mcp`; the **Web** package contains the same two binaries built with the off-by-default `web` feature, which compiles in the read-only Web server and embeds the frontend. Both are produced for linux x86_64 and macOS arm64/x86_64 (Windows is [a separate specification](wm-vfbqmw1r7x0j)), published as draft GitHub Release assets with SHA-256 checksums, a licence inventory, and a source tarball that carries the prebuilt frontend.

A tag `v<version>` drives the pipeline: it refuses to run unless the tag equals the workspace version, builds the frontend once, embeds that exact tree in every Web artifact, runs the release gates, and attaches everything to a draft Release. A human runs the release checks and publishes the draft; nothing is published automatically and nothing goes to crates.io.

## User Stories

1. As a Linux user, I download `waymark-v<version>-linux-x86_64.tar.gz`, verify its `.sha256`, extract it, and run `waymark version`, `waymark init`, `waymark new`, and `waymark list` without installing Rust.
2. As a macOS user on either architecture, I do the same with the matching `macos-aarch64` or `macos-x86_64` archive.
3. As a user who wants the Web view, I download the `waymark-web-…` archive, run `waymark serve`, and get the read-only view in a browser at a loopback URL.
4. As a basic-package user, running `waymark serve` tells me Web is not included and how to get a Web package, without downloading or installing anything.
5. As a basic-package user, I replace the binary with the Web package's and keep my `.waymark/` project and every work item unchanged.
6. As a user, `waymark version` reports the version and the capabilities included in this build.
7. As an agent or script, `waymark version --json` returns the same information plus the embedded frontend manifest in the versioned envelope.
8. As a user, I can verify that the Web view I downloaded was built from the release's source: the manifest's version and commit match the tag, and the served `index.html` hash matches the manifest.
9. As a user on macOS, the install documentation tells me the binary is unsigned and what Gatekeeper will do about it, instead of letting me discover it by failure.
10. As a user configuring an agent, `docs/mcp-clients.md` gives me working stdio configuration examples for Claude Code, DSH, and a generic client.
11. As a maintainer, pushing tag `v<version>` runs one workflow that builds the frontend once and every artifact from that tree.
12. As a maintainer, the workflow fails immediately if the tag does not equal the workspace version.
13. As a maintainer, a missing frontend, a failed frontend build, or a failed gate fails the release instead of producing an artifact.
14. As a maintainer, the workflow produces a draft Release holding both variants for every platform, each with a `.sha256`, plus the source tarball, and I review it before publishing.
15. As a maintainer, I edit the generated release notes to match `CHANGELOG.md` before publishing the draft.
16. As a maintainer, the release requires a real Rust 1.85 build, not just a declared `rust-version`.
17. As a maintainer, I can run the packaged artifacts through the same checks locally that CI ran, so a published draft is verified rather than assumed.
18. As a security-conscious user, I get a dependency-licence inventory in every archive and no silent network access at build or run time.

## Implementation Decisions

### Build variant

- `waymark-cli` gains `[features] web = ["dep:waymark-web"]` and `waymark-web = { workspace = true, optional = true }`. The default build is basic; `--features web` is the Web build. No `full` alias.
- The basic build must not compile `waymark-web` at all; a CI step asserts `cargo tree -p waymark-cli -e features` does not list it.
- `waymark serve` is a clap subcommand present in both builds. Its real implementation is behind `#[cfg(feature = "web")]`; the basic branch returns a typed error whose registered code and exit class are `WEB_NOT_INCLUDED` (validation class), with a message naming the Web package. The command never shells out to a downloader or installer.
- Asset embedding lives in `waymark-web`, so the basic build's dependency graph never sees it.

### `waymark serve`

- Surface: `waymark serve [--project DIR] [--port PORT] [--open]` plus the global `--json`, `--color`, `--project`, and `--lock-timeout-ms` flags.
- Project discovery matches the rest of the CLI: the current directory by default, walking up to the nearest `.waymark/config.toml`; `--project` sets the start directory. A missing or invalid project fails before the listener binds and never creates or modifies anything.
- Binds `127.0.0.1` only. The default port is `waymark_web::DEFAULT_PORT` (7331); `--port 0` selects a free port.
- A busy port is a `PORT_IN_USE` failure that names the port and suggests `--port 0`. Waymark does not silently fall back to another port.
- No browser is opened unless `--open` is given; on Linux and macOS it runs `xdg-open` or `open`, and a launch failure is a warning, not a serve failure.
- Readiness prints `serving http://127.0.0.1:<port>` on stderr in human mode; with `--json`, it prints `{"url": "http://127.0.0.1:<port>"}` as the envelope's data and then blocks serving requests.
- Read-only behaviour, Host/Origin rules, and Markdown safety are the Web specification's; this surface adds no write path and no second server.

### Version reporting

- `waymark version` prints `waymark <version> (<capabilities>)`, for example `waymark 0.1.0 (cli, mcp)` or `waymark 0.1.0 (cli, mcp, web)`.
- `waymark version --json` returns `{version, capabilities, assets}` in the versioned envelope; `assets` is the embedded manifest for a Web build and absent for a basic build.
- The embedded manifest is a generated `assets-manifest.json`: `{version, git_sha, build_id, files: [{path, sha256, size}]}`. `version` is the package version, `git_sha` comes from `WAYMARK_GIT_SHA` at build time (absent locally, required for a release build), and `build_id` is generated by the frontend build.

### Frontend assets and provenance

- The frontend build emits `crates/waymark-web/ui/build`, and a manifest generator writes `assets-manifest.json` beside it.
- `waymark-web/build.rs` embeds the tree — gzipped at build time, served with `Content-Encoding: gzip` and the correct `Content-Type` — and generates the manifest accessor. A Web build with a missing `index.html` or a missing manifest fails the build with instructions, never a placeholder.
- The build script does not run npm. Building the frontend is an explicit prior step (documented in the README and run by CI).
- `WAYMARK_SKIP_UI_BUILD=1` is accepted only to reuse a tree built in the same workflow; it does not disable the requirement, and the manifest must still be present.
- Release checks assert: manifest `version` equals the workspace version, `git_sha` equals the tagged commit, `index.html`'s hash matches the manifest entry, and a freshly started packaged `serve` returns an `index.html` whose hash matches. A mismatch fails with `ASSETS_VERSION_MISMATCH`.
- `waymark doctor` reports an embedded-manifest disagreement as unhealthy but does not block startup.

### Release pipeline

- `.github/workflows/release.yml` triggers on `v*` tags with `contents: write`.
- `version-gate`: read `[workspace.package] version` and fail unless it equals `${GITHUB_REF_NAME#v}`.
- `web-dist`: `actions/setup-node` with the frontend lockfile, `npm ci`, `npm run build`, manifest generation; uploads the built tree as an artifact and packs `waymark-web-dist-v<version>.tar.gz` + `.sha256` for the source tarball.
- `build` matrix: `ubuntu-latest`/`x86_64-unknown-linux-gnu`, `macos-latest`/`aarch64-apple-darwin`, `macos-15-intel`/`x86_64-apple-darwin`; downloads the built tree and builds both variants with `WAYMARK_SKIP_UI_BUILD=1` (Web) and without the feature (basic).
- Packaging: `waymark-v<version>-<os>-<arch>.tar.gz` and `waymark-web-v<version>-<os>-<arch>.tar.gz`, each holding `waymark`, `waymark-mcp`, `README.md`, `LICENSE-MIT`, `LICENSE-APACHE`, and `THIRD_PARTY_LICENSES.txt` from `cargo-about`; a `.sha256` sibling per archive. The base name is stable across the two variants only in its suffix; the variant prefix is part of the name.
- `source`: `git archive` of the tag plus the built frontend tree, packed as `waymark-src-v<version>.tar.gz` + `.sha256`.
- `release`: downloads every artifact, creates the Release as a **draft** with `generate_release_notes: true`, and attaches all archives, checksums, and the source tarball. Publishing is a manual step.
- The `build` job runs the Web artifact's provenance check on the runner before uploading, so a mismatch never reaches the draft.
- No `cargo publish` step exists anywhere in the pipeline.

### Release gates

- `.github/workflows/ci.yml` runs on pull requests: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features` on `ubuntu-latest` and `macos-latest`, the frontend's `npm ci && npm run check && npm run build`, and a build with a real Rust 1.85 toolchain.
- The release workflow repeats the same gates on the tagged commit before packaging; a red gate stops the release.
- `cargo metadata --locked` is part of the 1.85 job so a dependency raising its MSRV fails there rather than in a user's build.

### Documentation and changelog

- `README.md` gains an Install section: per-platform download, `.sha256` verification, extraction, the macOS Gatekeeper note for an unsigned binary, and the basic-to-Web upgrade (replace the binary; `.waymark/` is untouched).
- `docs/mcp-clients.md` documents `waymark-mcp` over stdio with Claude Code, DSH, and a generic JSON configuration example.
- `CHANGELOG.md` follows Keep a Changelog and is written by hand; the generated release notes are edited to match it before the draft is published.
- The design map [Waymark build, packaging, and distribution](wm-g2bzgcmzweaz) records the decisions this specification implements.

## Testing Decisions

- CI is the test: the gate jobs above must be green on the tagged commit, and the release workflow refuses to package when any is red.
- Assert the variant split: the basic build's dependency graph excludes `waymark-web`; the Web build's `waymark version` reports `web`; the basic build's `serve` returns `WEB_NOT_INCLUDED` with the documented exit class.
- Test `serve` over real HTTP with `--port 0`: `GET /` returns `index.html`, `_app/` assets resolve, non-GET returns 405, the Host/Origin guard rejects a foreign host, and the served `index.html` hash equals the embedded manifest entry.
- Test the readiness contract: human mode prints the URL to stderr, `--json` emits the envelope before serving, and a busy port fails with `PORT_IN_USE` naming the suggestion.
- Test `version` and `version --json` shapes, including the basic build's absent `assets` field and the Web build's manifest contents.
- Test the manifest generator as a pure function: stable file list, sorted paths, per-file SHA-256, and a failing exit when `index.html` is missing.
- Run `scripts/release/check-artifacts.sh` locally against downloaded draft artifacts; it verifies checksums, archive contents, binaries' reported version, and the Web provenance check, and CI runs the same script on `ubuntu-latest`.
- Keep the release checklist a checklist, not a test suite: anything checkable in code belongs in CI, and a finding from a manual run gets a regression test with its fix.

## Out of Scope

- Windows, which is [Waymark Windows platform support](wm-vfbqmw1r7x0j).
- Publishing crates to crates.io.
- Binary signing, Apple notarization, and SBOM generation.
- An install script, a Homebrew or other package-manager tap, and auto-update.
- musl targets and linux arm64.
- LAN or public binding, and non-loopback deployment.
- Web UI behaviour, MCP business behaviour, and any new product feature.
- A second Web server, a second frontend build, or a daemon.

## Further Notes

- Clove is the reference for the tag-triggered workflow, the draft-Release-then-publish order, cargo-about licence text, and the checksum convention. Waymark deliberately diverges on four points: no crates.io publishing, no automatic free-port fallback in `serve`, two named prebuilt packages instead of one feature-complete archive, and a source tarball that carries prebuilt assets instead of a published crate that downloads them at build time.
- The frontend is built once per release and every Web artifact embeds that same tree, so the binaries, the source tarball, and the provenance manifest cannot disagree.
- This specification is ready for implementation planning and ticket decomposition after publication.
