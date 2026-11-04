---
schema: 1
id: wm-7k3m4p9x2cqd
title: Waymark Web tree, category filtering, and graph overview
category: spec
status: open
owner: frank
creator: frank
created_at: 2026-11-04T00:57:47Z
updated_at: 2026-11-04T12:23:06Z
---
# Waymark Web Tree, Category Filtering, and Graph Overview

## Problem Statement

Waymark's read-only Web overview currently presents work items as a clear directory-style tree, but its category organization is incomplete for real project exploration. The combined view must remain a single cross-category hierarchy while making category identity visible. The category-grouped view needs explicit controls for selecting one or more categories rather than showing every category without choice. Users also need a graph view to understand hierarchy and dependencies spatially, without losing the existing tree's semantics, read-only safety, accessibility, or browser-local state behavior. The graph also needs a reading direction that fits the hierarchy on screen, without discarding the project's settled layout, viewport, selection, or accessibility state.

## Solution

Extend the existing read-only Web overview with category selection and a graph presentation alongside the existing tree presentation. Combined remains one hierarchy across categories. Category-grouped mode provides multi-select category buttons with select-all and clear-all actions. When a selected work item needs an ancestor from an unselected category to preserve its actual parent context, the ancestor remains visible as temporary context and is prominently marked as outside the selection.

Add a graph view backed by the same active tree data and relationships. Render parent-child relations as solid edges in a hierarchical layout that defaults to top-to-bottom, with a user-selected left-to-right alternative. When dependency visibility is enabled, render directed dependency relations as dashed edges with arrowheads. The graph is read-only, supports navigation and density controls, reuses the existing item detail, and offers a toggleable structured text/list alternative for keyboard and screen-reader users.

## User Stories

1. As a project member, I want the combined view to remain one cross-category hierarchy, so that parentage is never obscured by category boundaries.
2. As a project member, I want every work item in the combined view to show its category identity, so that I can distinguish categories without splitting the hierarchy.
3. As a project member, I want category identity to use the existing stable shape and category name cues, so that the visual language remains consistent with the tree view.
4. As a project member, I want to switch between the combined view and category-grouped view, so that I can choose global hierarchy or category organization.
5. As a project member, I want category-grouped view to show category selection buttons, so that I can control which categories are in scope.
6. As a project member, I want to select multiple categories at once, so that I can compare related categories without changing views repeatedly.
7. As a project member, I want a select-all action, so that I can restore the full category view quickly.
8. As a project member, I want a clear-all action, so that I can remove the current category scope quickly.
9. As a project member, I want selected categories to filter the grouped tree, so that unrelated work does not obscure the selected work.
10. As a project member, I want a selected work item's necessary unselected-category ancestors to remain visible, so that filtering does not falsify parentage.
11. As a project member, I want temporary context ancestors to be prominently marked as outside the selection, so that I do not mistake them for selected work.
12. As a project member, I want filtering to preserve the actual parent-child relationship, so that a selected child is not silently promoted to a false root.
13. As a project member, I want the same category selection semantics in the graph view, so that changing presentation does not change the meaning of my filter.
14. As a project member, I want an explicit empty state when no categories are selected, so that an empty canvas is not mistaken for a loading or failure state.
15. As a project member, I want an explicit empty state when selected categories contain no active work items, so that I understand why nothing is shown.
16. As a project member, I want an easy path from an empty filtered state to select all categories, so that I can recover without guessing.
17. As a project member, I want to switch between tree and graph presentations, so that I can use the clearest representation for the question I am asking.
18. As a project member, I want the graph to use the same active work-item data as the tree, so that the two views never disagree about what exists.
19. As a project member, I want parent-child relations shown as solid edges, so that hierarchy is the primary graph structure.
20. As a project member, I want dependency relations shown only when dependency visibility is enabled, so that the graph can remain readable when dependency information is not needed.
21. As a project member, I want dependency relations shown as dashed edges, so that I can distinguish them from parent-child edges.
22. As a project member, I want dependency edges to have direction arrows, so that I can tell which work item depends on which other work item.
23. As a project member, I want a concise graph legend, so that solid parent-child and dashed dependency semantics are immediately clear.
24. As a project member, I want the graph arranged hierarchically from top to bottom, so that it preserves the reading order of the directory tree.
25. As a project member, I want the graph to show the complete parent-child hierarchy on first entry, so that I can get a project-wide overview immediately.
26. As a project member, I want graph nodes to be collapsible and expandable, so that large projects remain manageable.
27. As a project member, I want graph collapse and expansion to affect presentation only, so that browsing never changes project data.
28. As a project member, I want to pan the graph canvas, so that I can inspect areas outside the current viewport.
29. As a project member, I want to zoom the graph, so that I can read dense or distant nodes.
30. As a project member, I want a fit-to-view action, so that the complete visible graph can be brought into view.
31. As a project member, I want a reset-viewport action, so that I can return to a predictable starting view.
32. As a project member, I want common touchpad and touch gestures to work for graph navigation, so that the graph is usable on modern input devices.
33. As a project member, I want graph nodes to be non-draggable, so that no accidental gesture can imply or mutate a relationship.
34. As a project member, I want clicking a graph node to open the existing read-only detail panel, so that details behave consistently across presentations.
35. As a project member, I want the selected graph node to be visibly marked, so that I can tell which item the detail panel describes.
36. As a project member, I want graph navigation to preserve the current project context when I open and close details, so that inspection does not reset my place.
37. As a project member, I want graph collapse and expansion state remembered per project, so that repeated visits do not discard my chosen density.
38. As a project member, I want graph collapse and expansion state kept separate from tree expansion state, so that optimizing one presentation does not damage the other.
39. As a project member, I want graph viewport state remembered per project, so that my pan and zoom position survives a presentation switch or reload.
40. As a project member, I want graph state isolated from other projects opened on the same browser origin, so that one project's preferences cannot affect another project.
41. As a project member, I want invalid stored graph node IDs ignored after project data changes, so that stale browser state cannot break the view.
42. As a project member, I want archived work items excluded from the graph entirely, so that archive acts as logical deletion in active views.
43. As a project member, I want archived work items excluded from unresolved graph entries and placeholders, so that archived records do not appear indirectly.
44. As a project member, I want missing-parent and parent-cycle items shown in a dedicated unresolved-relations area, so that data problems are visible without inventing hierarchy.
45. As a project member, I want unresolved entries to state their reason, so that I know why they are not in the normal hierarchy.
46. As a project member, I want unresolved items not treated as ordinary roots, so that the graph does not imply a valid parent structure.
47. As a keyboard user, I want a toggleable structured text/list alternative to the visual graph, so that I can access the same information without relying on visual edges.
48. As a screen-reader user, I want the graph alternative to expose node names, categories, parent-child relations, dependencies, and unresolved reasons, so that the graph's meaning remains available non-visually.
49. As a keyboard user, I want to select and inspect graph items using the same focus and detail behavior as the tree, so that the new presentation remains operable without a pointer.
50. As a project member, I want the graph and its alternative to remain strictly read-only, so that browsing cannot create, edit, archive, or move work items.
51. As a project member, I want unavailable browser storage to leave graph controls usable for the current session, so that persistence failure does not become a browsing failure.
52. As a project member, I want a concise notice when graph preferences cannot persist, so that the reset behavior is understandable.
53. As a project member, I want incomplete projections or damaged files to produce an explicit read failure rather than a misleading partial graph, so that I can trust the overview.
54. As a project member, I want valid work to remain visible when a complete inventory contains an unresolved parent reference, so that one bad relationship does not hide unrelated work.
55. As a project member, I want the existing read-only Web security boundaries to remain in force for the new view, so that graph content cannot bypass safe rendering or local-origin protections.

56. As a project member, I want a graph control to choose between top-to-bottom and left-to-right, so that I can read a wide hierarchy in the direction that fits it.
57. As a project member, I want top-to-bottom to be the default, so that the graph's first appearance is predictable.
58. As a project member, I want the direction control only in graph presentation, so that tree browsing is not cluttered with a control it cannot affect.
59. As a project member, I want one direction preference per project shared by the combined and grouped graph modes, so that switching views does not reset my choice.
60. As a project member, I want the direction preference restored when I refresh or reopen the project, so that repeated visits keep my reading direction.
61. As a project member, I want switching direction to preserve my category selection, selected item and open detail, graph expansion state, and dependency visibility, so that changing geometry does not discard my work.
62. As a project member, I want the visible graph fitted to the canvas after an explicit direction switch, so that the new orientation is immediately readable.
63. As a project member, I want parent-child relationships and sibling order preserved when the direction changes, so that the layout changes only geometry, not meaning.
64. As a project member, I want left-to-right parent-child lines to connect the parent's right side to the child's left side, and dependency arrows to keep their original direction, so that the relationship reading stays correct.
65. As a project member, I want the graph fitted to the remaining visible canvas when the detail panel is open, and the empty state retained when there is no graph, so that direction switching never hides content.
66. As a project member, I want direction switching to keep working for the current session when browser storage is unavailable, so that persistence failure does not disable the control.
67. As a project member, I want direction chosen explicitly rather than derived from hierarchy depth, item count, or viewport size, so that the layout does not change unexpectedly.
68. As a project member, I want a refresh or reopen to restore the last saved viewport for the current direction when possible, and otherwise fit the visible graph, so that the starting view is predictable.
69. As a keyboard and screen-reader user, I want the direction control operable by keyboard and to expose the selected direction without relying on icon or colour alone, so that I can choose and confirm it non-visually.

## Implementation Decisions

- Extend the existing Web overview rather than creating a second application or a second relationship model.
- Keep the Web adapter strictly read-only. No graph gesture, category control, or presentation state may issue a write request.
- Keep the shared core and existing tree API as the authority for active work items, parentage, dependencies, archive exclusion, unresolved relations, and computed state. The frontend may derive renderable graph elements and presentation filtering from the returned data, but must not invent domain semantics.
- Preserve the existing combined and grouped view names and add a presentation choice for tree versus graph. The combined presentation remains one cross-category hierarchy; grouped presentation remains category-oriented.
- Category selection is multi-select in grouped mode and is available to the graph when grouped filtering is active. Provide select-all and clear-all actions.
- A selected category scope includes matching active work items and any unselected-category ancestor required to preserve the selected item's actual parent chain. Context ancestors are presentation-only, are clearly marked as temporary context/outside selection, and are not counted as selected category results.
- Do not promote a filtered child to a fake root solely because its real parent is outside the selected category set.
- A no-selection or no-match result is an explicit empty state with a recovery action to select all categories. It is not an empty or interactive-looking canvas.
- Render parent-child edges as solid and dependency edges as optional dashed edges. Dependency edges are directed and use arrowheads. No other relationship types are included in the graph.
- Use a hierarchical layout with a graph-level direction choice between top-to-bottom (the first-use default) and left-to-right. Do not use force-directed layout for the initial feature. In left-to-right mode parents sit on the left and children on the right; parent-child lines connect the parent's right side to the child's left side, and enabled dependency arrows keep their original relationship direction. Expose the direction control in the graph's control area, never in tree presentation. The graph library is an implementation detail and may be Cytoscape.js, Svelte Flow, or another compatible library after a small integration evaluation; the user-visible contract must not depend on a library's internal API.
- Store one direction preference per project in browser-local state, shared by the combined and grouped graph modes and isolated by normalized project-root identity. Restore it on refresh or reopening; a different origin, browser, or project starts from the default. Direction is always chosen explicitly; never derive it from hierarchy depth, item count, or viewport size.
- Switching direction changes layout geometry only: preserve the category selection, selected work item and open detail panel, graph expansion/collapse state, and dependency visibility. After an explicit switch, fit the visible graph to the canvas; with the detail panel open, fit the remaining visible graph canvas; if there is no graph, keep the empty state and do not fit an empty graph. Refresh or reopen restores the last saved viewport for the current direction when a usable record exists, and otherwise fits the visible graph.
- When browser storage is unavailable, direction switching remains usable for the current session and the existing persistence notice explains that the preference cannot be saved; a later session may return to the default.
- The direction control is keyboard-operable and exposes the selected direction to assistive technology; icons or colour alone must not be the only way to distinguish its choices.
- Display the full parent-child hierarchy on first graph entry and fit it to the viewport. Provide per-node collapse/expand, pan, zoom, fit-to-view, and reset-viewport controls. Nodes cannot be dragged to alter relations.
- Node selection opens the existing read-only detail panel, visibly marks the selected node, and preserves the current presentation context when the detail closes.
- Provide a concise legend for solid parent-child edges and dashed dependency edges, and ensure the legend itself is accessible.
- Provide a user-toggleable structured text/list alternative for the graph. The alternative must expose the same visible nodes, categories, parent-child relations, dependency relations when enabled, and unresolved reasons without relying on SVG/canvas geometry.
- Treat archive as logical deletion for active Web presentation: archived items are absent from graph nodes, tree nodes, unresolved entries, dependency targets, and placeholders.
- Keep missing-parent and parent-cycle entries in the existing unresolved-relations area, with reasons, without creating virtual parents or treating them as ordinary roots. A damaged file or incomplete projection that prevents a complete tree remains an explicit read failure.
- Extend browser-local preference storage with presentation, selected category scope, graph collapse state, and graph viewport state. Namespace all state by normalized project-root identity and keep graph state separate from tree expansion and tree scroll state. Invalid or obsolete IDs are ignored.
- Keep the existing storage-failure behavior: in-memory session behavior continues and a concise persistence notice is shown.
- Reuse existing item-detail loading, focus-return, safe Markdown rendering, Host/Origin validation, and local-only service behavior.
- Prefer a component seam around the presentation model and graph rendering that can be tested through the public UI. Avoid exposing graph-library objects as application state or requiring tests to assert library internals.

## Testing Decisions

- Test externally visible behavior and stable UI/API contracts, not graph-library internals, CSS coordinates, or private component implementation details.
- Use the existing frontend UI seam with injected fetch and storage doubles for tree/graph rendering, category selection, persistence, detail selection, and storage failure behavior.
- Cover combined mode as one hierarchy with category identity cues and no category-section semantics.
- Cover grouped mode with single and multiple category selection, select-all, clear-all, context ancestors, prominent context marking, and no-match/clear-all empty states.
- Cover graph rendering with solid parent-child edges, optional dashed dependency edges, directed dependency arrowheads, legend content, stable category cues, and the hierarchical presentation contract in both top-to-bottom and left-to-right modes. Assert semantic roles/labels and data relationships rather than pixel positions.
- Cover direction switching: the top-to-bottom default, the control appearing only in graph presentation, one per-project preference shared across combined and grouped modes, restoration on refresh, an explicit switch preserving selection/detail/collapse/dependency visibility, fitting with and without the detail panel, the no-graph empty state, storage failure, keyboard operation and selected-direction announcement, and no automatic switching by depth, item count, or viewport size.
- Cover graph interaction with node selection, visible selected state, detail reuse, keyboard operation, non-draggable behavior, collapse/expand, pan/zoom controls, fit-to-view, reset, and the toggleable structured alternative.
- Cover graph and tree state persistence by project and presentation: category selection, tree expansion, graph collapse state, graph viewport state, direction, dependency visibility, selected presentation, scroll state, invalid stale IDs, unavailable storage, and write failures.
- Cover archive exclusion, missing-parent and parent-cycle unresolved displays, empty active projects, and explicit failure for incomplete projections or damaged files through the public HTTP/UI boundary.
- Cover that no graph or category interaction performs a non-GET request or modifies project data.
- Reuse existing frontend prior art for tree rendering, dependency overlays, details, browser storage isolation, empty states, unresolved lists, read failures, safe content rendering, and accessibility assertions.
- Use real HTTP tests for active tree data, malformed requests, Host/Origin restrictions, absence of writes, and safe content handling. The graph must consume the same read-only contract rather than introduce a graph-specific write or domain endpoint.
- If a graph library requires a browser-only runtime, isolate it behind the UI seam and keep semantic graph transformation tests independent of its rendering engine.
- Validate the selected dependency and graph-library versions with the frontend lockfile and existing build/check/test commands. Do not make successful tests depend on an external network at runtime.

## Out of Scope

- Any Web write operation, editing, archiving, restore, drag-to-reparent, drag-to-depend, or graph-based mutation.
- A force-directed layout, freeform node positioning, arbitrary user-authored layouts, layout editing, or automatic direction selection based on hierarchy depth, item count, or viewport size.
- Search, arbitrary query/filter replacement, custom-field-specific filtering semantics, or a general graph query language.
- Displaying archived work in the active Web graph or tree.
- Soft-link edges, custom-field reference edges, Git history edges, or any relationship other than parent-child and optional dependency edges.
- A separate graph API, second projection, second error taxonomy, or graph-specific domain rules.
- Formal packaging, embedded Web assets, distribution, installation, release automation, or `waymark serve` feature gating.
- Authentication, multi-user collaboration, live subscriptions, server-side preference synchronization, or cross-browser preference synchronization.
- Guaranteeing graph readability for an unbounded number of work items; large projects receive navigation and collapse controls but no promise of a single-screen overview.

## Further Notes

- The existing directory tree remains the primary overview for users who prefer explicit indentation; the graph is an additional presentation, not a replacement.
- The design map records the settled decisions that led to this specification, including the category context rule, graph edge semantics, hierarchy layout and direction, viewport controls, archive treatment, unresolved handling, accessibility alternative, and legend.
- A small prototype or spike may be used to compare Cytoscape.js and Svelte Flow against the required read-only hierarchical graph, but it must preserve the user-visible contract in this specification and should not expand scope into an editor.
- This specification is ready for implementation planning and ticket decomposition after publication.
