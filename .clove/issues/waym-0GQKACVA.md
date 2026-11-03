---
schema: 1
id: waym-0GQKACVA
title: Error registry, envelopes, and exit mapping
status: closed
type: feature
priority: 2
created: 2026-11-03T05:43:38.205Z
updated: 2026-11-03T16:50:39.408Z
closed: 2026-11-03T16:50:39.408Z
parent: waym-WMSPEC01
labels: [ready-for-agent]
deps: [waym-3Y19NH3K]
relates: []
duplicates: []
supersedes: []
---
## Parent

waym-WMSPEC01 — Waymark functional specification.

## What to build

Every failure from the existing commands becomes a typed, registered error carrying a stable code, its exit class, typed details, and an optional location. Machine responses use the versioned envelope on stdout, so a parser always receives valid JSON, while stderr stays human narrative. CLI exit codes come from one mapping table rather than per-command logic.

## Acceptance criteria

- [ ] every business error carries a registered code, its exit class, typed details, and an optional location union of work item, file, field, config key, or relationship
- [ ] JSON failure envelopes are written to stdout as `{v, ok: false, error}`, identical in shape across adapters
- [ ] bad flags and unknown subcommands exit 1 with adapter-level `USAGE_ERROR`, which is deliberately absent from the domain registry
- [ ] warnings such as an owner mismatch never change the exit code
- [ ] codes are coarse recovery units, and adding, removing, or reclassifying one requires updating the golden snapshot in the same change