# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-11-12

### Added

- `--cascade` on `archive`, `link add`, `link remove`, and `create`, and the
  same optional boolean on the MCP tools that map to them. It is the consent a
  command gives for writing a second work item, and it authorizes only the
  caller's own items.
- Two error codes for a cross-item write: `OWNED_BY_OTHER` (the other item
  belongs to someone else, so re-running cannot help) and
  `CASCADE_CONSENT_REQUIRED` (registered for callers that can supply consent;
  no current command raises it).
- `ONE_SIDED_SOFT_LINK`: `doctor` and `show` report a soft link that one side
  records and the other does not, naming the side that is missing it.
- `CLOSED_PARENT`: an active item whose direct parent is closed is labelled in
  `list` and `ready`, and reported as an informational finding by `doctor`.
  Nothing is written and no readiness changes.
- Archive results list the documents it left untouched and who still references
  the archived item, so retiring one item no longer hides who was affected.
- `create` and `link add` results report the documents they left untouched,
  with each partner's owner and whether it is the operator's.

### Changed

- Archive no longer rewrites another owner's document. It strips inbound
  references only from documents the operator owns, and leaves the rest in place
  with a `doctor` finding for its owner to resolve. `HAS_ACTIVE_CHILDREN` still
  refuses an item with live children.
- `link add` without `--cascade` no longer writes the other item. It records the
  link on the caller's item and reports the partner as one-sided; re-running
  from the other side completes the pair.
- `link remove` without `--cascade` removes the link from the caller's item
  only. Removing a link is a statement about one's own item; adding one is a
  claim about both.
- `create --link` without `--cascade` no longer rewrites the referenced item.
- Ownership for cross-item writes resolves the operator the same way everything
  else does: `.waymark/user.toml`, then Git `user.name`. A project with no
  operator refuses a cross-item write rather than silently skipping it.

### Removed

- Nothing. `CASCADE_CONSENT_REQUIRED` is deliberately kept registered without a
  producer so that a later operation can adopt it without a registry change.

### Breaking

- A command that rewrites a second document without `--cascade` now leaves that
  document alone; a caller relying on the old two-sided write must pass the flag
  and own the other item.

## [0.1.4] - 2026-11-11

### Added

- `waymark-mcp --ready` prints `OK` and exits 0 when a Waymark project is found
  above the working directory, and `--forget` exits 0 without doing anything.
  They exist so a CLI connector can ask "is this install usable here" and
  "log out" without a separate binary; Waymark has no accounts.
- `scripts/gitee-mirror-download.sh` and `scripts/gitee-release.sh` mirror a
  published release to Gitee, where the archives download far faster for users
  in mainland China than from github.com.

## [0.1.3] - 2026-11-11

### Added

- `server.json` at the repository root describes the MCP server for the
  official MCP Registry, pointing at the Cargo package `waymark-mcp` 0.1.3 over
  stdio.
- `waymark-mcp` has its own README on crates.io, covering installation, how the
  server finds a project, client configuration, and the full tool list.

### Changed

- The `waymark-mcp` crate description names what the server does instead of the
  internal architecture, and the crate now carries `repository`, `keywords`, and
  `categories` so it is findable from crates.io and docs.rs.
- CI refuses to pass when a published path dependency in
  `[workspace.dependencies]` declares a version other than the workspace
  version; the release workflow runs the same check ahead of the release. This
  is the failure mode where crates.io strips `path` and publishes a dependency
  version that is not in the index.

## [0.1.2] - 2026-11-08

### Added

- `waymark-core`, `waymark-web`, `waymark-mcp`, and `waymark-cli` are published
  to crates.io, so `cargo install waymark-cli` installs the `waymark` binary and
  `cargo install waymark-mcp` installs the MCP server. The CLI crate is named
  `waymark-cli` because `waymark` on crates.io is an unrelated crate.

### Changed

- The release workflow publishes the four crates in dependency order after a
  reviewer approves the `crates-io` deployment; a pre-release tag still produces
  a GitHub pre-release and no crates.io version.
- The README banner and the GitHub social preview use the new Waymark artwork;
  the 1280x400 README crop and the 1280x640 social card are both refreshed.

## [0.1.1] - 2026-11-07

### Changed

- The README opens with the pitch, a first-run transcript, the capabilities the
  tracker guarantees, and who should not use it, followed by the package and
  installation reference. `README.zh-CN.md` carries the same structure.
- The README landing section now includes a replaceable banner asset; the same
  1280x640 source also exports a 1280x400 README crop and a GitHub social preview.
- The read-only Web view shows the Waymark logo in the header, replacing the
  three-bar placeholder mark.

### Added

- `SECURITY.md`, covering the supported versions, the private reporting
  channel, the response targets, and the security scope of the CLI, the MCP
  server, and the Web view.
- `README.zh-CN.md`, a Simplified Chinese translation of the README, linked
  from the English one.
- Every release archive now ships `README.zh-CN.md` and `SECURITY.md`, so the
  links in the packaged `README.md` resolve.

### Fixed

- The first-run transcript in the README matches the output of `waymark init`,
  `waymark create`, and `waymark list`.

## [0.1.0] - 2026-11-06

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
- Prebuilt archives for Linux x86_64, macOS Apple silicon, macOS Intel, and
  Windows x86_64 (MSVC), named `waymark-v<version>-<os>-<arch>.tar.gz` and
  `waymark-web-v<version>-<os>-<arch>.tar.gz`, or the matching `.zip` on
  Windows, each with a `.sha256` sidecar, a README, both licence files, and
  `THIRD_PARTY_LICENSES.txt`.
- The Web view's cache and index status dashboard, opened from the header
  instead of occupying the page: projection completeness and freshness,
  lifetime cache counters, and a paged active-item index.
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
  verification, first commands, the macOS Gatekeeper note, the Windows
  SmartScreen note for unsigned binaries, and the basic-to-Web upgrade path.
- `docs/mcp-clients.md` with stdio configuration examples for Claude Code,
  DSH, and a generic JSON client.
- Dual licensing under MIT or Apache-2.0.

### Fixed

- A read-only `.waymark/.gitignore` now stops `waymark init` seeding and
  `waymark config set` with `LOCK_FAILED`; previously a local `user.toml` could
  be written even though the ignore file that should have tracked it could not
  be updated.

### Security

- Release artifacts carry SHA-256 checksums and a `cargo-about`
  dependency-licence inventory.
- The Web view is read-only, binds loopback only, and offers no LAN or public
  binding.
- The binaries do not download or install anything at run time.
- macOS binaries are unsigned and not notarized; Gatekeeper therefore requires
  explicit approval on first launch, as documented in the README. Windows
  binaries are unsigned too, so SmartScreen may require the same explicit
  approval.

[Unreleased]: https://github.com/QiYueran-source/waymark/compare/v0.1.4...HEAD
[0.1.4]: https://github.com/QiYueran-source/waymark/releases/tag/v0.1.4
[0.1.3]: https://github.com/QiYueran-source/waymark/releases/tag/v0.1.3
[0.1.2]: https://github.com/QiYueran-source/waymark/releases/tag/v0.1.2
[0.1.1]: https://github.com/QiYueran-source/waymark/releases/tag/v0.1.1
[0.1.0]: https://github.com/QiYueran-source/waymark/releases/tag/v0.1.0
