# One store-wide write lock, released by the kernel

Every mediated write — work items, relationships, archive, migration, and configuration — takes a single advisory lock on one file in the project metadata directory, held for one read-modify-write window and released by closing the descriptor. There is no stale-lock reclamation, no PID probe, no lease, and no holder metadata. A lock per work-item file cannot span the multi-file operations that need exclusivity (archive relationship cleanup, migration, multi-target relationship writes), and a lock that survives crashes needs staleness heuristics that fail on recycled PIDs, slow filesystems, and clock skew. The kernel already releases the lock on process exit, crashes included.

## Considered Options

- A lock per work-item file: rejected because those files are replaced by temporary file plus rename, binding the lock to a stale inode, and it cannot span a multi-file operation.
- A soft lock with heartbeat and stale reclaim: rejected because reclaim heuristics fail exactly in the crash case they exist for.
- No lock, relying only on content fingerprints: rejected because it cannot make prevalidation, staging, and rollback exclusive.

## Consequences

- Reads, `show`, dry runs, and previews never block on the lock.
- A long-lived server takes the lock per write and releases it before responding.
- Unix only, local filesystems only; network-filesystem concurrency is unsupported.
