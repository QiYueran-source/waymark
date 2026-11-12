# Waymark Domain

Waymark models project work items, their lifecycle, relationships, and human accountability in a local-first, Git-friendly tracker.

## Language

**Work item**:
A project-tracked unit of work with its own identity, lifecycle, and relationships.
_Avoid_: Issue (unless referring to a tracker-specific record), ticket (when the meaning is broader than a specific workflow).

**Category**:
A project-defined classification of work items. A category does not by itself define lifecycle or relationship semantics.
_Avoid_: Type (when it could be confused with a work item's intrinsic kind or schema).

**Owner**:
The human accountable for a work item. Ownership is distinct from who created or currently executes the work.
_Avoid_: Assignee (when accountability is meant), executor.

**Executor**:
The human or agent currently asked to perform work on a work item; a project may represent this through its own custom-field convention. Executor status is not ownership or authorization.
_Avoid_: Owner (when execution is meant), assignee (unless that is the project's explicit convention).

**Creator**:
The declared identity that created a work item, retained as provenance when ownership or execution changes. It is not proof of an authenticated actor.
_Avoid_: Owner, operator.

**Operator**:
The human identity a local project configuration declares as the one currently running Waymark-mediated operations. It identifies the caller for warnings, and a cross-item write additionally compares it with the other work item's owner. It is not authenticated identity, not a work item field, and does not grant any authority over a single work-item write.
_Avoid_: Authenticated user, actor (unless explicitly qualified as declared operator), owner.

**Parent**:
The single optional hierarchical relation from a work item to its containing work item. Parentage is independent of category and does not itself imply blocking or lifecycle inheritance.
_Avoid_: Dependency.

**Dependency**:
A directed hard-blocking relation expressing that one work item waits for another to be completed. Dependencies are distinct from parentage.
_Avoid_: Parent, prerequisite (unless direction is made explicit).

**Archive**:
An irreversible-through-Waymark logical deletion marker that removes a work item from ordinary active views while retaining its file, identity, and Git history; it is distinct from closing.
_Avoid_: Close, restore (as a Waymark operation).

**Closed**:
A core lifecycle state meaning the work item is completed and no longer blocks its dependents.
_Avoid_: Archived, deleted.

**Blocked**:
A derived condition in which a non-closed work item has one or more unfinished hard dependencies; it is not a lifecycle state.
_Avoid_: Status (when referring to this derived condition).

**Blocked by**:
The read-only list of hard-dependency references currently preventing a work item from being ready, including unresolved targets; unresolved targets are also identified as dangling dependencies.
_Avoid_: Persisted field (when describing the computed result).

**Ready**:
A derived condition indicating an open or in-progress work item has no unfinished or dangling hard dependencies.
_Avoid_: Status (when referring to this derived condition).

**Closed parent**:
A derived observation that an active work item's direct parent is in the closed state. It is a read signal, not a lifecycle state of the item itself, and it changes nothing.
_Avoid_: Unblocked, stale, cascade.

**Soft link**:
A non-blocking association between work items that does not affect readiness or lifecycle. It is stored on both work items that it relates, so it can exist on one side only while the other side has not been updated.
_Avoid_: Dependency.

**Cross-item write**:
A waymark-mediated write that changes a work item other than the one the command addresses, such as rewriting a soft-link partner or removing inbound references during archive. It requires explicit consent and an owner equal to the operator; a write that changes only its own target is not a cross-item write.
_Avoid_: Cascade (as a general verb for any change that touches several files).

**Consent**:
The explicit per-operation agreement a caller gives for a cross-item write, carried by the `cascade` option. It permits writing the caller's own other work items and never another owner's.
_Avoid_: Permission, authorization (those imply an authenticated privilege Waymark does not have).

**Custom field**:
A project-declared, uniquely named field for work-item data outside Waymark's stable core semantics. Its configured type defines only a generic value shape; it has no field-specific validation, relationship, authorization, or workflow behavior. It is optional unless the project schema explicitly requires it.
_Avoid_: Core field.

**Field type**:
A classification of a custom field's generic value shape, not a domain behavior or validation rule beyond the basic type check. Generic operations may filter or sort by the value without adding field-specific semantics.
_Avoid_: Reference semantics (for custom fields).

**Stable ID**:
The immutable, project-unique identity of a work item, independent of its category, name, and storage path.
_Avoid_: Name, slug, path.

## Storage and machine contract

**Format generation**:
The on-disk data format a Waymark release declares it can read and write. It changes only when the file format itself changes, and no configuration edit can raise it.
_Avoid_: Schema version (when the file format is meant), configuration revision.

**Configuration revision**:
The monotonic counter that increments once per effective mediated configuration write. It is informational only and never determines freshness, conflict detection, or cache invalidation.
_Avoid_: Version (unqualified), format generation.

**Projection**:
The disposable in-process view of work-item files, either complete and fresh or explicitly incomplete. It is rebuilt from files and is never a second source of truth.
_Avoid_: Index, cache (when the authoritative view is meant).

**Field projection**:
The caller-selected subset of fields a collection row renders, either a named set or all set fields, with unset values omitted. Unknown names are rejected; it changes the row shape, not which work items the query selects.
_Avoid_: Mask, view (when the field subset is meant).

**Archive range**:
The selector that decides whether a collection query reads active or archived work items. A single work item is read by stable ID regardless of range.
_Avoid_: Filter (when the active/archived scope is meant).

**Diagnostic finding**:
A data-integrity observation reported by validation or diagnostics, carrying a severity, a shared code, an optional location, and whether it is fixable. It is distinct from an operation that failed.
_Avoid_: Error (when no operation failed).

**Error code**:
The stable machine code that names the recovery action for a failed operation and is shared by every interface. Codes are coarse recovery units, not one per failure variant.
_Avoid_: Exit code (that is a class, not a code), message text.

**Write lock**:
The single store-wide advisory lock that serializes Waymark-mediated writes and is released by the operating system when the process exits. It is never held across calls.
_Avoid_: File lock (for a single work-item file), mutex.
