# Local development and the CI gates

[`.github/workflows/ci.yml`](../.github/workflows/ci.yml) is the authority on
what must pass. Every gate it runs has a local equivalent below, with the same
flags and shell, so a green run here predicts the pipeline. Two legs are not
reproducible on this host — the macOS runner and the Rust 1.85 toolchain — and
are marked as CI-only.

Run every command below from the repository root (the directory holding
`tickets/` and `waymark/`) unless a snippet changes directory.

## Prerequisites

- Rust stable. The workspace declares `rust-version = "1.85"`, and CI also runs
  the default workspace build and test suite on Rust 1.85.0.
- Node.js 22 and npm. The frontend job pins Node 22.

All Rust commands run from `waymark/` (the Rust workspace root). Frontend
commands run from `waymark/crates/waymark-web/ui`.

## Shared caches on this host

A sandboxed shell on this development host cannot write to `$HOME`, so the
shared caches live in the main checkout at `/home/frank/projects/waymark`:

```sh
export CARGO_HOME=/home/frank/projects/waymark/.cargo-home
export CARGO_TARGET_DIR=/home/frank/projects/waymark/.cargo-target
export npm_config_cache=/home/frank/projects/waymark/.npm-cache
```

These are local conveniences; CI uses the runner's defaults.

## Frontend job

```sh
cd waymark/crates/waymark-web/ui
npm ci
npm run check
npm run build
```

`npm run build` writes `waymark/crates/waymark-web/ui/build`. The embedding
build also requires the provenance manifest beside that tree, so run the
generator from the workspace root (the frontend job runs this from `waymark/`
with a `working-directory` override):

```sh
cd waymark
cargo run --locked -p waymark-web --bin waymark-web-manifest
```

CI uploads `build/` and `assets-manifest.json` as one artifact, and both Rust
matrix jobs download it into `waymark/crates/waymark-web/ui`, so the files land
exactly where `waymark-web/build.rs` expects them. Locally the commands above
already leave them in place. Check that both exist:

```sh
test -f crates/waymark-web/ui/build/index.html
test -f crates/waymark-web/ui/assets-manifest.json
```

## Rust job (`ubuntu-latest` and `macos-latest`)

From `waymark/`:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --workspace
```

The `--all-features` commands compile the `web` variant, which enables
`waymark-web`'s `embed` feature and therefore needs the frontend tree and
manifest from the previous section. The default `cargo test --workspace` does
not: the default build never reaches the embedding path.

The workflow runs each block below with `bash -e`; the smoke block toggles
errexit around the failure it expects. The dependency-graph assertion prints
nothing and succeeds when the basic graph is clean:

```sh
if cargo tree -p waymark-cli -e features | grep -q 'waymark-web'; then
  echo "::error::the basic waymark-cli dependency graph must not include waymark-web"
  exit 1
fi
```

The basic `serve` must exit 3 with a `WEB_NOT_INCLUDED` failure envelope, and
the Web build's `version` must report `web`:

```sh
project="$(mktemp -d)"
cargo run --quiet --locked -p waymark-cli --bin waymark -- --project "$project" init
set +e
output="$(cargo run --quiet --locked -p waymark-cli --bin waymark -- --json --project "$project" serve)"
status=$?
set -e
if [ "$status" -ne 3 ]; then
  echo "::error::basic serve exited $status, expected 3 (WEB_NOT_INCLUDED)"
  exit 1
fi
if ! printf '%s\n' "$output" | grep -q 'WEB_NOT_INCLUDED'; then
  echo "::error::basic serve did not report WEB_NOT_INCLUDED: $output"
  exit 1
fi
version="$(cargo run --quiet --locked -p waymark-cli --features web --bin waymark -- version)"
if ! printf '%s\n' "$version" | grep -q 'web'; then
  echo "::error::the Web version did not report the web capability: $version"
  exit 1
fi
```

The Web `version` output is `waymark 0.1.0 (cli, mcp, web)`.

## MSRV job (Rust 1.85) — CI-only on this host

The 1.85 toolchain is not installed on the development host, so this leg runs
only in CI. Install it to reproduce it locally:

```sh
rustup toolchain install 1.85.0
```

From `waymark/`:

```sh
cargo +1.85.0 metadata --locked
cargo +1.85.0 build --workspace --locked
cargo +1.85.0 test --workspace --locked
```

`cargo metadata --locked` refuses a lockfile that no longer matches the
manifests, so a dependency bump cannot slip past review. The `cargo build` and
`cargo test` steps are what enforce the declared `rust-version`: the compiler
rejects a dependency that requires a newer Rust before it reaches a user. This
leg uses the default feature set, so it needs no frontend tree.

## What only runs in CI

- The Rust matrix's `macos-latest` leg.
- The Rust 1.85.0 toolchain leg.
- Artifact transfer between jobs: locally the frontend tree already sits where
  the build script expects it; `actions/upload-artifact` and
  `actions/download-artifact` reproduce that handoff across isolated jobs.
