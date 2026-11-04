---
schema: 1
id: wm-vx6w5s4m9mqh
title: Define creation defaults and required inputs
category: map
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:20Z
updated_at: 2026-11-03T23:52:21Z
closed_at: 2026-11-03T23:52:20Z
parent: wm-y14hs95k8wct
---
## Question

What must a caller supply to create a work item, and what may Waymark derive?

Resolve:

- whether `owner` is required or defaults to the declared operator, and whether the same default applies to `creator`;
- whether `category` is required or falls back to a configured default;
- what happens when a declared custom field is `required` and the create request omits it: reject the create, or discover it later as a validation error;
- whether creation applies static custom-field defaults, and whether a supplied value always wins;
- whether a create request may specify the format generation or system timestamps (already answered: no — confirm nothing else is caller-settable);
- which of these defaults appear in CLI flags versus project configuration.
