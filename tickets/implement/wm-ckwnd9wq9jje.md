---
schema: 1
id: wm-ckwnd9wq9jje
title: link remove always works on one side
category: implement
status: closed
owner: frank
creator: Frank
created_at: 2026-11-12T06:27:26Z
updated_at: 2026-11-12T09:46:07Z
closed_at: 2026-11-12T09:46:07Z
parent: wm-62yfmh5j41t5
dependencies:
  - wm-0txgrh7efs7z
  - wm-363qccgxcw0q
---
## Parent

[Cross-item write consent, archive safety, and dangling references](wm-62yfmh5j41t5)

## What to build

Removing a soft link is always possible. Without consent it drops the link from my item only and succeeds, leaving the partner untouched; the partner keeps a reference that my `doctor` will report. With consent, and a partner that is mine, both sides are removed as before.

## Acceptance criteria

- [ ] `link remove` without consent succeeds and leaves the partner byte-identical
- [ ] `link remove --cascade` against a partner that is mine removes both sides
- [ ] `link remove --cascade` against another owner's item is refused with the foreign-owner code and writes nothing
- [ ] Removing a link that my item does not list still fails as it does today; consent does not make an absent link removable

## Blocked by

- wm-363qccgxcw0q — Give cross-item writes a place to say yes
- wm-0txgrh7efs7z — Refuse a missing consent and a foreign partner distinctly
