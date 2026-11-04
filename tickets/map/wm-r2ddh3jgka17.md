---
schema: 1
id: wm-r2ddh3jgka17
title: Define concurrency and lock lifecycle
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

How does the project write lock behave as an operational mechanism?

Fingerprint conflict detection and project-level locking are settled, but the lock's lifecycle is not:

- what the lock is (file, lock directory, OS-level advisory lock) and where it lives;
- how a caller waits, whether waiting is bounded, and what happens on timeout;
- how a lock left behind by a crashed process is detected and reclaimed without letting two writers in;
- whether a long-lived MCP server holds the lock between calls or takes it per write;
- what a reader observes while a writer holds the lock: block, read stale data with a marker, or fail;
- whether lock contention produces a retryable structured error and which code.
