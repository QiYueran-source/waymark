---
schema: 1
id: waym-J3VX6HQS
title: Define concurrency and lock lifecycle
status: closed
type: feature
priority: 2
created: 2026-11-03T03:25:00.000Z
updated: 2026-11-03T04:05:25.777Z
closed: 2026-11-03T04:05:25.777Z
assignee: DeepSeek Agent
parent: waym-VS5Z5543
labels: [wayfinder:grilling]
deps: []
relates: []
duplicates: []
supersedes: []
---
## Question

How does the project write lock behave as an operational mechanism?

Fingerprint conflict detection and project-level locking are settled, but the lock's lifecycle is not:

- what the lock is (file, lock directory, OS-level advisory lock) and where it lives;
- how a caller waits, whether waiting is bounded, and what happens on timeout;
- how a lock left behind by a crashed process is detected and reclaimed without letting two writers in;
- whether a long-lived MCP server holds the lock between calls or takes it per write;
- what a reader observes while a writer holds the lock: block, read stale data with a marker, or fail;
- whether lock contention produces a retryable structured error and which code.
