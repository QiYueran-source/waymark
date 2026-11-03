---
schema: 1
id: wm-2gc9r3qmthc4
title: Define the configuration command surface and its migration boundary
category: map
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:20Z
updated_at: 2026-11-03T23:52:22Z
closed_at: 2026-11-03T23:52:20Z
parent: wm-y14hs95k8wct
---
## Question

Which project settings are editable through ordinary configuration commands, and which must go through a migration? The migration decisions now claim storage root, category path, and category key as `migrate storage` actions, but the CLI/MCP interface design document still lists `config_set_storage_root`, `category_update`, and `category_move_storage` under configuration. The same effect currently has two candidate entrances.

Resolve:

- the complete set of configuration operations and their names, and which are absent by design;
- where the boundary sits between "configuration write" and "migration", and what makes an operation land on one side;
- how a category is added, renamed, re-pathed, and removed, including the rule that a category referenced by work items cannot simply disappear;
- whether configuration writes participate in the same fingerprint, locking, and atomicity rules as work-item writes;
- how this interacts with the resolved rule that adding a custom field is a configuration operation while removing one is `migrate schema --remove-field`.
