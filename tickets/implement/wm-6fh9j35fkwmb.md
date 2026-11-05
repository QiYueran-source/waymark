---
schema: 1
id: wm-6fh9j35fkwmb
title: Local user configuration document and effective resolution
category: implement
status: closed
owner: frank
created_at: 2026-11-05T09:02:22Z
updated_at: 2026-11-05T11:18:34Z
closed_at: 2026-11-05T11:18:34Z
---
## Parent

[Split shared and local Waymark configuration](wm-qkssvky32ptz)

## What to build

A collaborator can read their own effective operator, time zone, and cache settings from a separate ignored local document, and every adapter sees identical values and sources. The shared project configuration no longer carries those three settings at all.

The local document is a dedicated TOML shape accepting only the operator, the time zone, and the cache settings; unknown keys and shared project keys are rejected as configuration errors rather than silently ignored. Reading resolves local-first: an operation-level operator override, then an explicit local value, then a fallback of Git's effective `user.name` for the operator, the current machine-local zone for the time zone, and the built-in defaults for the cache. An absent Git identity leaves the operator unset instead of inventing one, and Git being unavailable or the project being outside a repository only makes that one fallback unavailable. An undetectable machine zone falls back to UTC and carries an informational warning, so the result is deterministic and visible. An absent local document is normal and is never created by a read.

`config show` presents the shared and local configuration separately, with the effective values and a per-field source naming the local document, Git, the system zone, or a built-in default. Human output has separate project and local sections; machine output carries separate project, local, effective, and source structures.

## Acceptance criteria

- [ ] The shared configuration schema rejects the operator, time zone, and cache keys and no longer writes them; a repository whose shared configuration still carries them is reported as invalid rather than silently accepted.
- [ ] A local document is written and read at the fixed metadata-directory location, accepting only `operator`, `timezone`, and the cache settings; every other key is a `CONFIG_INVALID` finding naming the offending key.
- [ ] Effective values and their sources are exposed by one shared core boundary, and the CLI `config show` renders them without re-deriving precedence.
- [ ] Operator resolution is operation override, then local value, then Git's effective `user.name`, then unset; a missing name leaves the operator unset and an operation still succeeds.
- [ ] Time-zone resolution is local value, then machine-local zone, then UTC with an informational warning; an explicit local value never consults system detection.
- [ ] Cache resolution is explicit local values, then built-in defaults; a written cache snapshot stays stable when program defaults later change.
- [ ] Git unavailability, absence of a repository, or an unreadable `user.name` degrades only the operator fallback and fails no ordinary operation.
- [ ] An absent local document is never created by a read, and an omitted creator still defaults to the effective operator when one is available.
- [ ] The repository's own `.waymark/config.toml` is migrated to the new shape, with its operator, time zone, and cache values moved into a local document, and the project validates clean.
- [ ] Core, CLI, and adapter-parity tests cover the accepted shape, rejected keys, every fallback branch, the UTC warning, cache snapshot stability, and source rendering.

**Blocked by:** None (can start immediately).

**Status:** ready-for-agent
