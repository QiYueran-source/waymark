---
schema: 1
id: wm-k3netb2kb3a4
title: Route local scalars to the local document
category: implement
status: closed
owner: frank
created_at: 2026-11-05T09:02:40Z
updated_at: 2026-11-05T11:53:46Z
closed_at: 2026-11-05T11:53:46Z
---
## Parent

[Split shared and local Waymark configuration](wm-qkssvky32ptz)

## What to build

The familiar configuration commands keep working, but the local scalars now land in the local document instead of the shared one.

`config set operator`, `config set timezone`, and the cache scalars write the local document, use the same project lock, validation, atomic replacement, and conflict detection as any other configuration write, and leave the shared configuration revision unchanged because no shared content changed. Setting an empty operator removes the local override and restores the Git fallback rather than acting as a special opt-out. Project configuration operations continue to update only shared fields, so category and custom-field work never lands in a local file. A manual edit of the local document is a supported path and is read the same way a mediated write is.

Writing the local document refuses when it is already tracked by Git, instructing the user to untrack it, and repairs a missing ignore entry first. The dry-run form reports what would change without writing anything, and a malformed or invalid local document is refused rather than overwritten.

## Acceptance criteria

- [ ] `config set operator`, `config set timezone`, and every cache scalar write the local document and never the shared one.
- [ ] A local write leaves the shared configuration revision unchanged, while an effective shared write still increments it exactly once.
- [ ] Clearing the local operator removes the override and restores the Git fallback instead of clearing the effective operator outright.
- [ ] Category and custom-field commands continue to write only shared configuration.
- [ ] Local writes take the project lock, validate completely, replace atomically, and detect a conflicting fingerprint.
- [ ] A missing ignore entry is repaired before the write; a tracked local document is refused with untracking guidance; an unsafe ignore file refuses the write.
- [ ] Dry run writes nothing and reports the intended change.
- [ ] A malformed or invalid local document fails the write rather than being overwritten.
- [ ] Core and CLI tests cover the routing, revision isolation, clearing-to-fallback, the tracked-file refusal, dry run, and conflict detection.

**Blocked by:** wm-9nztkqbezy49 — initialization creates and ignores the local document.

**Status:** ready-for-agent
