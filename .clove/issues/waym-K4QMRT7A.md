---
schema: 1
id: waym-K4QMRT7A
title: Define the configuration command surface and its migration boundary
status: open
type: feature
priority: 2
created: 2026-11-03T03:25:00.000Z
updated: 2026-11-03T03:25:00.000Z
parent: waym-VS5Z5543
labels: [wayfinder:grilling]
deps: []
relates: []
duplicates: []
supersedes: []
---
## Question

Which project settings are editable through ordinary configuration commands, and which must go through a migration? The migration decisions now claim storage root, category path, and category key as `migrate storage` actions, but the CLI/MCP interface design document still lists `config_set_storage_root`, `category_update`, and `category_move_storage` under configuration. The same effect currently has two candidate entrances.

Resolve:

- the complete set of configuration operations and their names, and which are absent by design;
- where the boundary sits between "configuration write" and "migration", and what makes an operation land on one side;
- how a category is added, renamed, re-pathed, and removed, including the rule that a category referenced by work items cannot simply disappear;
- whether configuration writes participate in the same fingerprint, locking, and atomicity rules as work-item writes;
- how this interacts with the resolved rule that adding a custom field is a configuration operation while removing one is `migrate schema --remove-field`.
