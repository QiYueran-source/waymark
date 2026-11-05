---
schema: 1
id: wm-0jfnex3j130v
title: Define cross-platform lock safety contract
category: map
status: open
owner: frank
created_at: 2026-11-05T00:53:09Z
updated_at: 2026-11-05T00:53:20Z
parent: wm-4wbvzsv34g2p
dependencies:
  - wm-78d0sa7jaxpb
---
## Type

grilling

## Question

What safety and concurrency guarantees must the project lock provide on Windows and Unix, including symlinks/reparse points, contention, process crashes, failures and the existing Windows specification’s check-then-open race?
