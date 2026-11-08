# Release checklist

A Waymark release is one version bump, one tag, one workflow run, and two human
decisions to publish. The workflow attaches everything to a **draft** GitHub
Release and stops there; the crates.io job in the same run waits behind a
required reviewer on its environment. Nothing reaches users until a human acts,
and this checklist is that human part.

The machine-checkable parts live in `.github/workflows/release.yml` and
`scripts/release/`; do not duplicate them here. A finding from a manual run gets
a regression test with its fix, not another checklist item.

## Before tagging

- [ ] `waymark/integration` is merged and its CI gates are green on the commit
      that will be tagged.
- [ ] `[workspace.package] version` in `waymark/Cargo.toml` is the version being
      released. Every crate inherits it, but the two published path dependencies
      repeat it, so bump all three places; `scripts/release/version.sh --check`
      fails until they agree, and the `Versions` CI job runs the same check.
- [ ] `CHANGELOG.md` has a dated entry for the version under `## [Unreleased]`,
      in Keep a Changelog form. The generated release notes will be reconciled
      with this section, so it must be accurate before the tag exists.
- [ ] `README.md`'s Install table and archive names still match the artifacts
      this version produces.
- [ ] The macOS Gatekeeper note still says the binaries are unsigned and not
      notarized, and the Windows note still says they are unsigned and may
      trigger SmartScreen.
- [ ] `docs/mcp-clients.md` still describes the shipped `waymark-mcp` binary.

Run the same gate the workflow will run, on the tagged commit:

```sh
cd waymark
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --workspace
cd crates/waymark-web/ui
npm ci
npm run check
npm run build
cd ../../../..
cargo run --locked -p waymark-web --bin waymark-web-manifest -- --version "$(scripts/release/version.sh)"
```

The last two commands need `WAYMARK_GIT_SHA` and `WAYMARK_RELEASE=1` only when
you are rehearsing a release build. A local packaging rehearsal is:

```sh
CARGO_HOME=... CARGO_TARGET_DIR=... scripts/release/package.sh \
  --version "$(scripts/release/version.sh)" \
  --git-sha "$(git rev-parse HEAD)" \
  --out dist
scripts/release/check-artifacts.sh --dir dist --require-platforms  # after all legs exist
```

`cargo-about` is required for a local package; install it with
`cargo install --locked --features cli cargo-about`.

## Tag and watch the pipeline

- [ ] Create an annotated tag `v<version>` on the release commit and push it.
- [ ] `version-gate` fails immediately if the tag does not equal
      `[workspace.package] version`. Do not “fix” this by editing the tag name
      after the fact; delete the tag and re-tag the correct commit.
- [ ] `web-dist` installs from the frontend lockfile, runs `npm run check` and
      `npm run build`, generates `assets-manifest.json`, and packs the tree.
- [ ] `gates` is green on `ubuntu-latest`, `macos-latest`, and
      `windows-latest` (`fmt`, `clippy` with `-D warnings`, tests with and
      without all features).
- [ ] `msrv` is green on a real Rust 1.85 toolchain, including
      `cargo metadata --locked`.
- [ ] `licences` produced a non-empty `THIRD_PARTY_LICENSES.txt` from
      cargo-about. A dependency with an unreviewed licence fails here; review it
      and update `scripts/release/about.toml` only with the licence text read.
- [ ] `build` is green for all four legs: `linux-x86_64`, `macos-aarch64`,
      `macos-x86_64`, and `windows-x86_64` (MSVC, `.zip`). Each leg runs the
      packaged Web `serve --port 0` provenance check before uploading; an
      `ASSETS_VERSION_MISMATCH` stops the release.
- [ ] `source` produced `waymark-src-v<version>.tar.gz` with the built frontend
      tree and the same manifest.
- [ ] `verify` ran `check-artifacts.sh` on `ubuntu-latest` against the
      downloaded archives. Linux binaries were executed; macOS and Windows
      archives were checked for checksum and contents there and executed on
      their own build legs.
- [ ] `release` created a **draft**; it did not publish.

If any job fails, stop. Fix the cause, delete the draft and the tag, and re-tag
only after the gates are green.

## Review the draft Release

- [ ] Open the draft Release and confirm it is a draft, not published.
- [ ] Confirm the asset list contains exactly:
      `waymark-v<version>-linux-x86_64.tar.gz`,
      `waymark-v<version>-macos-aarch64.tar.gz`,
      `waymark-v<version>-macos-x86_64.tar.gz`,
      `waymark-v<version>-windows-x86_64.zip`,
      `waymark-web-v<version>-linux-x86_64.tar.gz`,
      `waymark-web-v<version>-macos-aarch64.tar.gz`,
      `waymark-web-v<version>-macos-x86_64.tar.gz`,
      `waymark-web-v<version>-windows-x86_64.zip`,
      `waymark-src-v<version>.tar.gz`, and a `.sha256` for each.
      The intermediate `waymark-web-dist-*` archive is not a release asset.
- [ ] Reconcile the generated notes with `CHANGELOG.md`: replace the generated
      list with the changelog's Added/Changed/Fixed/Security wording for this
      version, add the installation pointer, and remove generated noise that
      does not describe a user-visible change.
- [ ] For an `-rc.N` release, confirm the pre-release flag is set.
- [ ] Download every asset to one directory and run the artifact check with the
      tagged commit:

```sh
mkdir -p dist
gh release download "v<version>" --dir dist --repo <owner>/waymark   # from the draft
scripts/release/check-artifacts.sh \
  --dir dist \
  --version "<version>" \
  --git-sha "$(git rev-parse "v<version>^{commit}")" \
  --require-platforms
```

`check-artifacts.sh` skips executing foreign-platform binaries on the host and
says so; run it on Linux for the Linux archives, on macOS for the macOS ones, and
on Windows for the `.zip` ones when you want every binary executed locally. The
build jobs already did that.

## Packaged smoke checks

Use the extracted archive, not the checkout.

- [ ] `sha256sum --check waymark-v<version>-linux-x86_64.tar.gz.sha256` (macOS:
      `shasum -a 256 -c ...`; Windows: compare `Get-FileHash` with the sidecar)
      prints `OK` or matches.
- [ ] `./waymark version` prints `waymark <version> (cli, mcp)` from the basic
      package (`.\waymark.exe version` on Windows).
- [ ] `./waymark version` prints `waymark <version> (cli, mcp, web)` from the
      Web package.
- [ ] `./waymark-mcp --version` prints `waymark-mcp <version>`.
- [ ] In a scratch directory, `./waymark init`, `./waymark create`, and
      `./waymark list` work without Rust or npm installed.
- [ ] The basic package's `./waymark serve` fails with `WEB_NOT_INCLUDED` and
      does not bind a port.
- [ ] The Web package's `./waymark serve --port 0` prints
      `serving http://127.0.0.1:<port>` on stderr, and `curl http://127.0.0.1:<port>/`
      returns the UI over HTTP.
- [ ] `./waymark version --json` exposes `assets.version` equal to the tag,
      `assets.git_sha` equal to the tagged commit, and an `index.html` hash that
      equals `sha256sum` of the file the server returns.
- [ ] On macOS, confirm the documented Gatekeeper approval still applies to the
      unsigned binary.
- [ ] On Windows, confirm the unsigned `.exe` binaries still launch through
      SmartScreen's “More info → Run anyway” path.

## Publish to crates.io

`publish-crates` runs after `verify` and publishes `waymark-core`, then
`waymark-web`, `waymark-mcp`, and `waymark-cli` in dependency order, waiting for
each version to reach the index before the next. It is skipped for a pre-release
tag: `v0.1.0-rc.1` produces a GitHub pre-release and no crates.io version.

One-time repository setup:

- [ ] The `crates-io` environment exists and requires at least one reviewer.
      Without a reviewer there is no gate, and the job publishes on its own.
- [ ] `CARGO_REGISTRY_TOKEN` is an environment secret on `crates-io`, not a
      repository secret, and the token carries only `publish-new` and
      `publish-update`.
- [ ] The crates.io account behind the token has a verified email address.
      Until it does, every upload fails with `A verified email address is
      required to publish crates to crates.io`.

Per release:

- [ ] Wait for the run to pause at *Waiting for review* on `publish-crates`;
      the draft Release is created in parallel. Approve only once the draft's
      artifacts passed the smoke checks above. The approval *is* the publish:
      the upload starts on approval, there is no second command.
- [ ] Rejecting the deployment cancels the job and leaves the draft in place;
      publishing the draft by hand then ships binaries without crates.
- [ ] Confirm the job reports all four crates and that
      `https://crates.io/crates/waymark-cli` shows the new version.
- [ ] In a scratch directory, `cargo install waymark-cli --version <version>
      --locked` installs it and `waymark version` prints that version.

A local run publishes for real; rehearse with `--dry-run` first. The script is
idempotent, so a run that stopped halfway resumes instead of failing on an
already-uploaded version:

```sh
scripts/release/publish-crates.sh --version <version> --dry-run
CARGO_REGISTRY_TOKEN=... scripts/release/publish-crates.sh --version <version>
```

Publishing is irreversible: a wrong version is yanked, never replaced.

## Publish

- [ ] Publish the draft (GitHub: *Edit* → *Publish release*).
- [ ] Download one archive and its `.sha256` from the published URL and verify
      the checksum again.
- [ ] Confirm `CHANGELOG.md` links the published tag and that the Release notes
      match the changelog for this version.
- [ ] Announce the release.

If a published artifact is wrong, publish a new patch release; do not silently
replace an asset under an existing tag. Note the problem in `CHANGELOG.md` and
the Release notes.
