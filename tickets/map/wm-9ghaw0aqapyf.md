---
schema: 1
id: wm-9ghaw0aqapyf
title: Define cross-platform verification and release gates
category: map
status: closed
owner: frank
created_at: 2026-11-05T00:53:10Z
updated_at: 2026-11-05T06:33:36Z
closed_at: 2026-11-05T06:33:36Z
parent: wm-4wbvzsv34g2p
dependencies:
  - wm-0jfnex3j130v
  - wm-31nx004kv0xy
  - wm-a3ewqxebssk1
  - wm-dq6f226w3g5k
  - wm-pkn47knm5t0n
  - wm-r6bmw1xrk5mn
  - wm-rn893kzbg524
  - wm-w69renycvb2p
---
## Type

grilling

## Question

What native CI, integration, safety, packaging and release checks must pass for Linux/macOS and Windows MSVC/GNU; what test exceptions require explicit reporting, and how is the MSVC-only Windows artifact verified?

## Platform facts gathered

- GitHub-hosted `windows-latest` currently resolves to Windows Server 2025 with Visual Studio 2026; the image preinstalls Rust, Clippy and Rustfmt, and its setup script runs `rustup target add x86_64-pc-windows-gnu`. MinGW lives at `C:\mingw64` (gcc 15.2.0, binutils 2.46, UCRT runtime). MSYS2 is installed at `C:\msys64` but is not on `PATH` and carries no mingw-w64 packages beyond its base system.
- The image enables Developer Mode (`AllowDevelopmentWithoutDevLicense=1`) and the runner itself runs as an administrator with UAC disabled, so symlink creation is expected to work there rather than being unavailable.
- The x64 workspace is on `D:\` and every mounted working volume is NTFS; no ReFS or Dev Drive is in use.
- `windows-gnu` carries upstream risk: rustc documents a MinGW-w64 default of MSVCRT while the image's toolchain is UCRT, and recorded failures involve `dlltool` (rust#128944, rust#155591) that are not fully resolved.
- Windows also has recorded spurious file-sharing failures during test runs (cargo#12042, rust-lang/rust#127883, runner-images#10483), which GitHub classified as a non-image issue.

## Decisions confirmed so far

- Confirmed by the user on 2026-11-05: both Windows targets run natively in CI on `windows-latest` — one MSVC leg and one GNU leg — each running `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`. Cross-compilation from Linux never substitutes for either leg, because the behaviour under test is rename atomicity, locking, ACL handling and reparse-point handling. The MSVC leg blocks merges from the start. The GNU leg is required as well but may begin as an observational, non-blocking job while the toolchain is validated by the pilot; it must be promoted to a blocking gate once the pilot passes, and the promotion is part of closing this work. Every run records the toolchain versions actually used, because the image's MinGW runtime differs from the one rustc documents.
- Confirmed by the user on 2026-11-05: a skipped test must be explicit and allowed. Each skip point carries a stable identifier and a reason, registered in a version-controlled allowlist; a skip listed there counts as a recorded coverage gap, and a skip not listed there fails that leg. At least one environment must actually execute the capability-dependent cases; on GitHub-hosted Windows the enabled Developer Mode and administrator runner are expected to satisfy this for symlink handling, so a skip there is a defect to investigate rather than a normal outcome. If no environment can run a case, the resulting capability exception must be stated in the release notes rather than left implicit.
- Confirmed by the user on 2026-11-05: the first Windows release publishes MSVC artifacts only — `x86_64-pc-windows-msvc` zip archives with checksums, passing the same version gate, licence gate and MSRV gate as the existing Linux and macOS legs. The GNU leg builds and tests but publishes nothing. Publishing a GNU artifact later requires it to pass the full set of gates rather than being exempted as a compatibility-only build.
- Confirmed by the user on 2026-11-05: the supported-platform statement is deliberately narrow. Windows is supported on NTFS local volumes only; network shares, FAT, exFAT and any other filesystem are unsupported and fail closed rather than degrading. Linux and macOS support is not expanded by this work. Git's minimum supported version is the lowest version that actually passes in CI, recorded with that version and stated in the release notes; only verified attribute combinations are promised, and an unverified combination produces an explicit error instead of a guessed result.
- Confirmed by the user on 2026-11-05: a failing gate is never converted into a pass by retrying it. Where the recorded Windows sharing-violation behaviour makes a test nondeterministic, the response is to make the test deterministic — not to add retries — and a quarantined test must carry a tracking reference and a deadline. Quarantine may not include the safety-critical cases: lock safety, replacement and rollback, migration, or path validation. A flaky safety test means the platform is not yet verified, not that the test is inconvenient.

## Remaining open

- The concrete allowlist entries and the exact toolchain versions pinned by the GNU leg after the pilot.
- Whether the GNU leg's MinGW needs MSYS2-provided binutils rather than the image's `C:\mingw64`, which only the pilot can settle.
- Unicode 16.0.0 data and Rust 1.85 compatibility evidence, filesystem and path-length limits, and the Git version and attribute matrix — all of which are measured by these gates rather than decided in advance.

## Resolution

The user confirmed the gate policy above on 2026-11-05. No gate described here has run: CI currently has no Windows leg, the GNU leg exists only as a plan, and no MSVC artifact has been produced. This ticket records required behaviour and evidence obligations, not a verified platform.

## Handoff

Specification reconciliation is the remaining step; it needs explicit authorization and is tracked by [Reconcile cross-platform decisions with existing specifications](wm-bpz94xwr28qa.md).
