# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-04

The first public release of Waymark.

### Added

- `waymark`, a local-first work-item tracker that stores each work item as a
  Markdown document with YAML frontmatter under the project's storage root and
  keeps project configuration in `.waymark/`.
- Project initialisation and work-item reads and writes: `init`, `create`,
  `show`, `title`, `body`, `field`, `owner`, `move`, `status`, `start`,
  `close`, `reopen`, and `archive`.
- Relationships and structure: `parent`, `link`, `dependency`, `tree`, and
  `dep`, including hierarchy and dependency trees.
- Collection reads: `list`, `ready`, `blocked`, `query`, `search`, and `stats`,
  with shared filtering, sorting, paging, projection, and archive ranges.
- Project operations: `project`, `git`, `config`, `migrate`, `doctor`,
  `validate`, and the generated capability reference `agent-doc`.
- `waymark-mcp`, a Model Context Protocol server over stdio that exposes the
  same project operations to agents.
- Two prebuilt variants from one tag: the basic package (`waymark` and
  `waymark-mcp`) and the Web package with the read-only Web view.
- Prebuilt archives for Linux x86_64, macOS Apple silicon, and macOS Intel,
  named `waymark-v<version>-<os>-<arch>.tar.gz` and
  `waymark-web-v<version>-<os>-<arch>.tar.gz`, each with a `.sha256` sidecar,
  a README, both licence files, and `THIRD_PARTY_LICENSES.txt`.
- `waymark serve`, available in both builds: the Web package serves the
  read-only view on loopback at port 7331 by default, with `--port` and
  `--open`; the basic package reports `WEB_NOT_INCLUDED` without downloading
  or installing anything.
- `waymark version` reports the release version and the capabilities compiled
  into the build; `waymark version --json` adds the embedded frontend manifest
  for a Web build.
- Embedded frontend provenance: an `assets-manifest.json` records the version,
  commit, build id, and per-file SHA-256 hashes, and the release checks compare
  it with the served `index.html`.
- Installation documentation with per-platform archives, checksum
  verification, first commands, the macOS Gatekeeper note for unsigned
  binaries, and the basic-to-Web upgrade path.
- `docs/mcp-clients.md` with stdio configuration examples for Claude Code,
  DSH, and a generic JSON client.
- Dual licensing under MIT or Apache-2.0.

### Security

- Release artifacts carry SHA-256 checksums and a `cargo-about`
  dependency-licence inventory.
- The Web view is read-only, binds loopback only, and offers no LAN or public
  binding.
- The binaries do not download or install anything at run time.
- macOS binaries are unsigned and not notarized; Gatekeeper therefore requires
  explicit approval on first launch, as documented in the README.

[Unreleased]: https://github.com/frank/waymark/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/frank/waymark/releases/tag/v0.1.0
