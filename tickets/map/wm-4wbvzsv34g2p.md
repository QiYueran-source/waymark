---
schema: 1
id: wm-4wbvzsv34g2p
title: Waymark cross-platform adaptation
category: map
status: open
owner: frank
created_at: 2026-11-05T00:53:09Z
updated_at: 2026-11-05T01:57:39Z
---
## Destination

Reach an implementable cross-platform adaptation design and a basis for revising the existing Windows specification. Identify decisions and verification gates; this map does not implement product code or synchronize specifications or implementation tickets without user approval.

## Notes

- Planning baseline: Linux and macOS existing targets; Windows x86_64 MSVC and GNU both built and tested in CI, with MSVC release artifacts only initially. This is a goal, not a claim of current support.
- Cover core filesystem, Git, CLI, MCP, Web, CI and release compatibility without adding product features. Local filesystems only; concurrent NFS/SMB access is unsupported. Windows ARM64, Cygwin and WSL-specific behavior are out of scope.
- Existing Windows specification: [Waymark Windows platform support](../spec/wm-vfbqmw1r7x0j.md). Review it rather than creating a competing specification. The older Unix-only functional scope is historical; decide how later changes relate to it.
- Waymark requires an owner and has no configured labels or claim field: decision type is in each ticket body; use native parent and dependency relations. One AI works sequentially for now; do not alter status solely to claim. Revisit claims if concurrent work is needed.
- Consult grilling and domain-modeling for human decisions and codebase-design for module design. Research uses the research skill, local research branches only, no automatic push or PR. Results are linked from tickets.

## Decisions so far

- [Research Windows filesystem safety primitives](wm-78d0sa7jaxpb.md): Evidence on lock safety, replacement, Windows metadata and cross-volume preflight; native Windows behavior remains untested.
- [Research Windows targets and release tooling](wm-31nx004kv0xy.md): Evidence on MSVC/GNU tooling, CI, runtime dependencies, packaging and browser launch; no Windows builds were run.

- [Define cross-platform lock safety contract](wm-0jfnex3j130v.md): Fail-closed lock-path safety and a shared cooperative lock protocol, with explicit threat limits and native validation.

## Not yet specified

- Further compatibility cases, limitations, or trade-offs uncovered by research and decisions that cannot yet be stated as precise questions.
- Any additional handoff details exposed by resolving the known decisions; do not rewrite existing specs yet.

## Out of scope

- Product features unrelated to platform compatibility; implementation while charting this map.
- Windows ARM64, Cygwin, WSL-specific behavior, and concurrent use over network filesystems.
- Separate Windows GNU release artifact initially.
