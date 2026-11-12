---
schema: 1
id: wm-xe5h8fgb7np9
title: doctor reports archived references and one-sided links
category: implement
status: closed
owner: frank
creator: Frank
created_at: 2026-11-12T06:27:27Z
updated_at: 2026-11-12T09:45:45Z
closed_at: 2026-11-12T09:45:45Z
parent: wm-62yfmh5j41t5
dependencies:
  - wm-9wz141hh9p73
---
## Parent

[Cross-item write consent, archive safety, and dangling references](wm-62yfmh5j41t5)

## What to build

`doctor` becomes the place a stale reference surfaces. It reports a dependency, a parent, or a soft link that points at an archived item, one finding per relation kind per item, so the reader knows which kind of repair applies. A soft link that exists on one side only is reported under its own code, telling "the partner never recorded it" apart from "the partner is gone". No finding carries a runnable repair command.

## Acceptance criteria

- [ ] A dependency, parent, or soft link pointing at an archived item produces one finding per relation kind, located on the referencing item and naming the archived target
- [ ] A one-sided soft link produces a finding under its own registered code, never the archived-target code
- [ ] `doctor` remains read-only: running it changes no business document
- [ ] No finding exposes a command a caller could execute as a repair; the machine output is diagnosis only
- [ ] `doctor` on a clean project still reports no findings

## Blocked by

- wm-9wz141hh9p73 — Archive rewrites nothing but its target
