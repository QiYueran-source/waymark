---
schema: 1
id: wm-wr11be4s3vpd
title: Define project-level custom field types and schema syntax
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

What finite custom-field type catalog and configuration syntax should Waymark provide for project-level fields, including required/default/enum/list/reference constraints, while keeping frontmatter strict and Git-friendly?

Context: custom fields are project-global, uniquely named, optional by default, and unknown undeclared keys are errors. Clove has no custom-field schema to copy; it only offers a strict fixed-field frontmatter model.
