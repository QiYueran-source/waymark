---
schema: 1
id: wm-rn893kzbg524
title: Define platform adaptation module design
category: map
status: closed
owner: frank
created_at: 2026-11-05T00:53:10Z
updated_at: 2026-11-05T05:38:55Z
closed_at: 2026-11-05T05:38:55Z
parent: wm-4wbvzsv34g2p
dependencies:
  - wm-0jfnex3j130v
  - wm-a3ewqxebssk1
  - wm-w69renycvb2p
---
## Type

grilling

## Question

Given the agreed platform contracts and research, where should platform-specific seams be placed, what small interfaces and adapters should exist, and which existing dependencies should be reused? Avoid assuming a new crate or public trait.

## Decisions confirmed so far

- Confirmed by the user on 2026-11-05: each supported platform target (Linux, macOS, Windows MSVC, Windows GNU) declares its platform capabilities at compile time rather than probing them on every operation. Runtime code reports only a declared capability that failed in practice. A capability declared absent fails closed for the operations that require it; no silent degradation is permitted. Capabilities that depend on the actual filesystem rather than the platform, at minimum volume identity and reparse-point identification, remain per-operation checks: even where declared, a single inconclusive judgement is treated as unknown and the operation refuses.
- Confirmed by the user on 2026-11-05: a maintained official Windows binding may be added as a `cfg(windows)` target-only dependency with a minimal feature set, because owner/DACL preservation, reparse-tag inspection, handle-based volume identity and no-replace moves are not expressible with the standard library alone. Refusing them would make editing permission-restricted files impossible on Windows, which is not an acceptable product outcome. The dependency must not raise `rust-version = "1.85"` or change the licence position; the exact crate, version and the required API list are fixed when the change is made.
- Confirmed by the user on 2026-11-05: the platform layer lives inside `waymark-core` as an internal module organised by capability, not as a new crate and not as a new public trait. `waymark-core`'s public API stays as it is; the module absorbs the scattered `cfg` branches and raw `libc` constants. A separate crate is deferred until a second consumer exists.
- Confirmed by the user on 2026-11-05: response-shape details are a separate decision, not part of this module design. The registry code and details schema for the directory-synchronization warning, the mapping of the three recovery states onto the existing `applied`, `failed` and `rollback_failed` detail lists, and the lossless non-UTF-8 path representation are tracked in [Define platform response interface details](wm-r6bmw1xrk5mn.md), which depends on this item.
- Confirmed by the user on 2026-11-05: fault injection is limited to optional hooks on the platform primitives, following the existing `Options`-closure style used by `LoadOptions::with_inventory_hook`; no general mock filesystem is introduced. Injected failures verify that rollback and reporting paths execute; real atomicity, permission and reparse semantics are verified natively on each platform. Test doubles must not become evidence that a platform behaves correctly.

## Current seams (evidence)

- Replacement is already concentrated in `write.rs`; only three primitives are platform-relevant: destination inspection, staging, and persist with directory synchronization.
- Lock acquisition touches the platform through one open call; the backoff and timeout logic is portable.
- Path validation has four parallel implementations (`config.rs` relative-path grammar, `ops/migrate.rs` checked_relative_path, `git.rs` path filter, and three separate `fn absolute` helpers) and five independent `symlink_metadata` checks. This is the largest and lowest-risk consolidation.
- Migration owns directory relocation in `ops/migrate.rs`, while `ops/move.rs` implements a second, non-rename write-then-delete relocation. The two must not be silently unified: the item move carries fingerprint semantics that a raw rename cannot express.
- Git is the most concentrated: one executable lookup and one process spawn. Its Windows branch currently only differs by the `git.exe` name and by returning an empty untracked diff, which contradicts the agreed patch contract.
- No filesystem, process or environment abstraction exists; only `Clock` and `IdSource` are injected, and every fault test depends on POSIX permission bits or symlinks.

## Implementation requirements for the Windows pilot

The user will run the first native Windows verification by hand; these requirements are what that pilot must be able to exercise. They are requirements, not an implementation plan, and they do not create implementation tickets.

1. `waymark-core` must compile for `x86_64-pc-windows-msvc` and `x86_64-pc-windows-gnu`. The unconditional `libc::ELOOP` and `libc::EXDEV` uses must be replaced by capability-based error classification that does not name POSIX constants outside `cfg(unix)`; the classification must not assume Windows uses the same numeric or symbolic error codes, and its exact mapping is established against native evidence.
2. Every test that calls `std::os::unix` must be gated or moved into a platform-specific test module so that `cargo test` compiles on Windows instead of failing at compile time. A Windows run that skips a case must report the skip with a reason; silently absent coverage is not acceptable.
3. Untracked single-item diffs on Windows must produce an applicable new-file patch as already decided, replacing the current silent empty result.
4. One relative-path grammar implementation must serve configuration validation and migration prechecks, and one path-inspection implementation must serve the five existing `symlink_metadata` sites. The agreed portable rules, Unicode 16.0.0 comparison and the reparse-point policy must be added there, not in parallel copies.
5. Lock opening must reject link and reparse final components and classify an unsafe open as a terminal lock failure, sharing the same safe-open primitive as replacement. The `.gitignore` write that currently precedes lock acquisition must be kept out of that primitive.
6. The replacement primitives must expose: no-follow destination inspection, staging whose temporary file is never more accessible than the target, persist with no unsafe in-place fallback, and post-persist directory synchronization that reports a warning on a failed attempt and nothing on a declared-absent capability. Windows execution must preserve owner and DACL and refuse a read-only target.
7. Storage migration must obtain volume identity from handles, refuse an unknown or different volume, perform no-replace relocation, and clean up only directories it created whose identity it can still confirm.
8. Git executable discovery must work on Windows, including the executable-name and search-path differences, and must not treat a missing executable and a failing command as the same condition.
9. The browser opener moves out of the CLI adapter into the platform module so that no platform behaviour remains in an adapter.
10. The pilot must run the cross-platform test surface with `cargo fmt --check`, `clippy -D warnings` and `cargo test` for both Windows targets, and record which cases ran, which were skipped and why. Passing Linux tests says nothing about Windows.

## Remaining open

- Exact crate, version and API list for the Windows binding.
- Concrete mechanism choices inside each primitive, and whether MSVC and GNU share one capability declaration or differ.
- Native evidence for every declared capability, including the directory-synchronization declaration.
- The response-shape decision tracked in [Define platform response interface details](wm-r6bmw1xrk5mn.md).

## Resolution

The user confirmed the module design decisions above on 2026-11-05 and asked for the Windows implementation requirements to be written down so they can run the first native verification themselves. Those requirements are recorded above; they are not an implementation plan and authorize no code change, no specification synchronization and no implementation tickets.

## Handoff

Response shapes move to [Define platform response interface details](wm-r6bmw1xrk5mn.md). Native evidence, CI provisioning, capability exceptions and the MSVC-only release artifact remain with [Define cross-platform verification and release gates](wm-9ghaw0aqapyf.md). The Windows pilot described above is manual and provisional: its outcome must be recorded as evidence, and an agreed behaviour that proves infeasible on Windows returns to the user rather than being quietly weakened.
