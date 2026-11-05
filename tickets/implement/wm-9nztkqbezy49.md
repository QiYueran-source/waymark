---
schema: 1
id: wm-9nztkqbezy49
title: Initialization creates and ignores the local document
category: implement
status: closed
owner: frank
created_at: 2026-11-05T09:02:32Z
updated_at: 2026-11-05T11:53:46Z
closed_at: 2026-11-05T11:53:46Z
---
## Parent

[Split shared and local Waymark configuration](wm-qkssvky32ptz)

## What to build

A new project is usable on the machine that created it without further setup, and re-initializing an existing project never disturbs anyone's preferences.

First initialization writes the local document with the effective Git `user.name`, the current machine-local zone, and the current cache defaults written explicitly, so the file is a complete snapshot rather than something that drifts when program defaults change. A missing Git name leaves the operator out of that snapshot. The initialization operator and time-zone options seed the new document when it is being created and have no effect on an existing one. Initializing an already-initialized project stays a no-op even when seeds are supplied: it neither creates a missing local document nor overwrites an existing one.

Initialization also ensures the metadata directory's ignore file lists the local document, alongside the existing write-lock entry. Waymark never touches the repository-level `.gitignore` or `.git/info/exclude`. An ignore file that is invalid, not writable, or a symbolic link fails the operation before any local settings file is created, so preferences cannot be committed by accident. If the local document is already tracked, initialization reports that and leaves the Git index alone.

## Acceptance criteria

- [ ] First initialization creates the local document with the Git identity when available, the machine zone, and explicit cache defaults, and the resulting project validates clean.
- [ ] Initialization with a missing Git identity omits the operator instead of inventing one.
- [ ] Initialization operator and time-zone options seed only a newly created document.
- [ ] Re-initializing an already-initialized project creates and overwrites nothing, with or without seeds, and leaves an existing local document byte-identical.
- [ ] The metadata ignore file gains the local-document entry on creation, and a missing entry is repaired without duplicating an existing one.
- [ ] Repository-level `.gitignore` and `.git/info/exclude` are never modified.
- [ ] An invalid, non-writable, or symlinked ignore file fails initialization before any local settings file exists.
- [ ] A tracked local document is reported and left untouched, with the Git index unmodified.
- [ ] Core and CLI tests cover first initialization, missing Git identity, seeds, the repeated-initialization no-op, ignore-entry creation and repair, the unsafe ignore-file refusals, and the tracked-file report.

**Blocked by:** wm-6fh9j35fkwmb — local user configuration document and effective resolution.

**Status:** ready-for-agent
