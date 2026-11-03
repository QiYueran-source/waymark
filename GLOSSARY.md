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
The human identity a local project configuration declares as the one currently running Waymark-mediated operations. It is advisory only: it identifies the caller for reminders and warnings, never authorizes or blocks a write, and is neither authenticated identity nor a work item field.
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

**Soft link**:
A non-blocking association between work items that does not affect readiness or lifecycle.
_Avoid_: Dependency.

**Custom field**:
A project-declared, uniquely named field for work-item data outside Waymark's stable core semantics. Its configured type defines only a generic value shape; it has no field-specific validation, relationship, authorization, or workflow behavior. It is optional unless the project schema explicitly requires it.
_Avoid_: Core field.

**Field type**:
A classification of a custom field's generic value shape, not a domain behavior or validation rule beyond the basic type check. Generic operations may filter or sort by the value without adding field-specific semantics.
_Avoid_: Reference semantics (for custom fields).

**Stable ID**:
The immutable, project-unique identity of a work item, independent of its category, name, and storage path.
_Avoid_: Name, slug, path.
