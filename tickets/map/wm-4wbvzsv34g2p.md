---
schema: 1
id: wm-4wbvzsv34g2p
title: Waymark cross-platform adaptation
category: map
status: open
owner: frank
created_at: 2026-11-05T00:53:09Z
updated_at: 2026-11-05T04:04:29Z
---
## Destination

Reach an implementable cross-platform adaptation design and a basis for revising the existing Windows specification. Identify decisions and verification gates; this map does not implement product code or synchronize specifications or implementation tickets without user approval.

## Notes

- Planning baseline: Linux and macOS existing targets; Windows x86_64 MSVC and GNU both built and tested in CI, with MSVC release artifacts only initially. This is a goal, not a claim of current support.
- Cover core filesystem, Git, CLI, MCP, Web, CI and release compatibility without adding product features. Local filesystems only; concurrent NFS/SMB access is unsupported. Windows ARM64, Cygwin and WSL-specific behavior are out of scope.
- Existing Windows specification: [Waymark Windows platform support](../spec/wm-vfbqmw1r7x0j.md) was absorbed into [Waymark functional specification](../spec/wm-srxj8ztv1zfz.md) on 2026-11-05 and closed; the functional specification is the single normative document. The older Unix-only functional scope is historical.
- Waymark requires an owner and has no configured labels or claim field: decision type is in each ticket body; use native parent and dependency relations. One AI works sequentially for now; do not alter status solely to claim. Revisit claims if concurrent work is needed.
- Consult grilling and domain-modeling for human decisions and codebase-design for module design. Research uses the research skill, local research branches only, no automatic push or PR. Results are linked from tickets.

## Decisions so far

- [Research Windows filesystem safety primitives](wm-78d0sa7jaxpb.md): Evidence on lock safety, replacement, Windows metadata and cross-volume preflight; native Windows behavior remains untested.
- [Research Windows targets and release tooling](wm-31nx004kv0xy.md): Evidence on MSVC/GNU tooling, CI, runtime dependencies, packaging and browser launch; no Windows builds were run.

- [Define cross-platform lock safety contract](wm-0jfnex3j130v.md): Fail-closed lock-path safety and a shared cooperative lock protocol, with explicit threat limits and native validation.

- [Define cross-platform file replacement and migration guarantees](wm-a3ewqxebssk1.md): Atomic single-file visibility, fail-closed permissions and migration, honest recovery and explicit durability limits.

- [Define cross-platform paths and Git behavior](wm-w69renycvb2p.md): Portable path syntax and Unicode comparison, fail-closed link and migration safeguards, scoped Git behavior and applicable untracked-file patches; native feasibility and interface details remain for follow-up.

- [Define filesystem warning and recovery response contract](wm-pkn47knm5t0n.md): Per-event synchronization warnings carried in the success metadata, an explicit platform capability declaration instead of per-write noise, and three-state recovery reporting shown to humans as well as machines.

- [Define platform adaptation module design](wm-rn893kzbg524.md): Compile-time capability declarations with per-filesystem checks, an internal capability-organised module, a narrow Windows binding, limited fault injection, and the requirements the first manual Windows pilot must exercise.
- [Define platform response interface details](wm-r6bmw1xrk5mn.md): A single warning code with registered details, three-state rollback reporting that replaces the ambiguous lists, and a tagged union that keeps UTF-8 paths readable while representing non-UTF-8 paths losslessly.
- [Define safe temporary and recovery file diagnostics](wm-dq6f226w3g5k.md): Configuration-derived candidate locations, unconfirmed coverage reported instead of assumed clean, and two read-only finding codes that reuse the existing diagnostic surface.

- [Define cross-platform verification and release gates](wm-9ghaw0aqapyf.md): Native MSVC and GNU legs with no cross-compilation substitute, an explicit skip allowlist, MSVC-only publishing, narrow filesystem support, and a no-retry rule for safety gates.

- [Reconcile cross-platform decisions with existing specifications](wm-bpz94xwr28qa.md): One surviving normative document, synchronized before any implementation ticket, with superseded text rewritten in place and the frozen registry updated with its snapshot in the same commit.

## Not yet specified

- Nothing remains to decide before the destination is reached. Every branch of the design tree was visited: lock safety, replacement and migration guarantees, portable paths and Unicode comparison, Git scope and patch behaviour, warning and recovery responses, response interface details, temporary and recovery diagnostics, the platform module design, the verification and release gates, and the reconciliation approach.
- The specification synchronization decided under Decisions so far has been carried out: [Waymark functional specification](../spec/wm-srxj8ztv1zfz.md) now states platform behaviour, [Waymark distribution, packaging, and release](../spec/wm-4125jxq7bpq8.md) carries the Windows legs and artifact, and the former Windows specification is absorbed and closed. What remains is work, not a decision: cut implementation tickets from the synchronized specification in dependency order, then obtain native Windows evidence.

## Out of scope

- Product features unrelated to platform compatibility; implementation while charting this map.
- Windows ARM64, Cygwin, WSL-specific behavior, and concurrent use over network filesystems.
- Separate Windows GNU release artifact initially.
