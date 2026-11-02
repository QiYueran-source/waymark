---
schema: 1
id: waym-0Y4VRHNA
title: Define project-level custom field types and schema syntax
status: closed
type: feature
priority: 2
created: 2026-11-02T06:14:40.890Z
updated: 2026-11-02T07:28:14.064Z
closed: 2026-11-02T07:28:14.064Z
assignee: DeepSeek Agent
parent: waym-VS5Z5543
labels: [wayfinder:grilling]
deps: []
relates: []
duplicates: []
supersedes: []
---
## Question

What finite custom-field type catalog and configuration syntax should Waymark provide for project-level fields, including required/default/enum/list/reference constraints, while keeping frontmatter strict and Git-friendly?

Context: custom fields are project-global, uniquely named, optional by default, and unknown undeclared keys are errors. Clove has no custom-field schema to copy; it only offers a strict fixed-field frontmatter model.