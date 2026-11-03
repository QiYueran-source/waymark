# Archive is an irreversible logical deletion

Waymark retires work through archive, which marks a work item as logically deleted while keeping its file, identity, relationships as historical context, and Git history. It is not a lifecycle status and Waymark cannot undo it: the file stays, is excluded from ordinary views, and returns only by checking out a historical version outside the tool. A `cancelled` status would have kept retired work inside every lifecycle query and invited accidental revival; physical deletion would have destroyed the only record of it.

## Considered Options

- A `cancelled` or `dropped` lifecycle status: rejected because retired work would stay in normal queries and every consumer would special-case it.
- Physical deletion: rejected because the authoritative record must survive.
- An unarchive command: rejected because it would have to reconstruct relationships that archive removed, while Git already provides a deliberate, reviewable restore.

## Consequences

- Archiving must clear the archived target's inbound core relationships in active files, in one all-or-nothing batch.
- Anything that needs a "deleted" concept in reporting has to read the archive range rather than a status.
