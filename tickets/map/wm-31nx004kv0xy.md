---
schema: 1
id: wm-31nx004kv0xy
title: Research Windows targets and release tooling
category: map
status: closed
owner: frank
created_at: 2026-11-05T00:53:09Z
updated_at: 2026-11-05T00:58:31Z
closed_at: 2026-11-05T00:58:31Z
parent: wm-4wbvzsv34g2p
---
## Type

research

## Question

What are the documented requirements and trade-offs for x86_64 Windows MSVC and GNU Rust targets, native builds and tests in GitHub Actions, runtime dependencies, and packaging or launching the browser safely? Identify practical CI limitations and primary sources without prescribing an unverified implementation.

## Research context

Completed in local branch `research/waymark-platform-toolchains` at commit `136cb7aac95ab87bfefdf6e777bd0bc62ed34dac`: [Windows targets and release tooling](../../.worktrees/research-platform-toolchains/research/windows-toolchains.md). The branch is not pushed or merged.

## Resolution

Current Rust documentation lists both Windows x86-64 targets as Tier 1; GNU carries an upstream maintenance warning. The existing CI, Unix-only tests, release scripts and browser opener need Windows-specific review. DLL dependencies must be established from actual artifacts and clean-client checks; a static CRT flag alone is not proof of no runtime dependencies. The evidence covers safe opener candidates and CI/release trade-offs. No Windows builds, runtime tests or artifact inspection were performed; no further design choice was made.
