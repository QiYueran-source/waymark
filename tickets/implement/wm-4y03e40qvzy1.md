---
schema: 1
id: wm-4y03e40qvzy1
title: create does not rewrite its link partner by surprise
category: implement
status: closed
owner: frank
creator: Frank
created_at: 2026-11-12T06:27:27Z
updated_at: 2026-11-12T10:20:24Z
closed_at: 2026-11-12T10:20:24Z
parent: wm-62yfmh5j41t5
dependencies:
  - wm-0txgrh7efs7z
  - wm-363qccgxcw0q
---
## Parent

[Cross-item write consent, archive safety, and dangling references](wm-62yfmh5j41t5)

## What to build

Creating an item with an initial soft link no longer edits the item it points at unless that is asked for and allowed. Without consent the new item records the link on its own side; with consent, and a partner whose owner, the new item's owner, and the operator all agree, the partner records the link too. When a partner was rewritten, the creation result says so.

## Acceptance criteria

- [ ] `create` with a soft link and no consent leaves the partner byte-identical
- [ ] `create` with consent rewrites a partner only when the new item's owner, the partner's owner, and the operator agree
- [ ] `create` with consent against a partner owned by somebody else leaves that partner byte-identical and names it in the result; `create` never refuses because a partner was left alone
- [ ] A partner that was rewritten appears in the result together with the documents written; a partner left alone is named as such
- [ ] A rewritten partner is named in the result's `partners` list, and `create` emits no `OWNER_MISMATCH` for it: under the three-identity rule a rewritten partner is always the operator's own document, so the warning could never be truthful

## Blocked by

- wm-363qccgxcw0q — Give cross-item writes a place to say yes
- wm-0txgrh7efs7z — Refuse a missing consent and a foreign partner distinctly
