---
schema: 1
id: wm-q90azj65xneg
title: 'Archive: irreversible logical deletion'
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:21Z
updated_at: 2026-11-03T23:52:22Z
closed_at: 2026-11-03T23:52:21Z
parent: wm-srxj8ztv1zfz
dependencies:
  - wm-9mxwsjy5vag9
---
## Parent

wm-srxj8ztv1zfz — Waymark functional specification.

## What to build

Retiring work becomes possible as an irreversible logical deletion: archive keeps the file and its metadata, clears the archived target's inbound core relationships from active files in one batch, and removes the item from ordinary views without touching Git history.

## Acceptance criteria

- [ ] archive is allowed from any status, changes neither status nor `closed_at`, adds `archived_at` and an optional `archive_reason`, and does not update `updated_at`
- [ ] re-archiving returns `ALREADY_ARCHIVED` without replacing either marker
- [ ] an item with an unarchived child is rejected with `HAS_ACTIVE_CHILDREN` and its child list, and archive never cascades
- [ ] archived items reject title, body, custom-field, status, owner, and relationship writes with `ITEM_ARCHIVED`
- [ ] inbound dependencies, inbound parent, and inbound soft links are removed from active files as one all-or-nothing batch that updates their `updated_at`
- [ ] archived items leave active lists, trees, search, ready, blocked, and statistics; reading one by stable ID still works and an archived-only range selects only archived items
- [ ] a dry run performs every check, reports each planned relationship removal, and writes nothing
