---
schema: 1
id: wm-7k3m4p9x2cqd
title: Waymark Web tree, category filtering, and graph overview
category: spec
status: closed
owner: frank
creator: frank
created_at: 2026-11-04T00:57:47Z
updated_at: 2026-11-04T12:57:16Z
closed_at: 2026-11-04T12:57:16Z
---
# Waymark Web Tree, Category Filtering, and Graph Overview

## Problem Statement

Waymark's read-only Web overview currently presents work items as a clear directory-style tree, but its category organization is incomplete for real project exploration. The combined view must remain a single cross-category hierarchy while making category identity visible. The category-grouped view needs explicit controls for selecting one or more categories rather than showing every category without choice. Users also need a graph view to understand hierarchy and dependencies spatially, without losing the existing tree's semantics, read-only safety, accessibility, or browser-local state behavior. The graph also needs a reading direction that fits the hierarchy on screen, without discarding the project's settled layout, viewport, selection, or accessibility state.

The current page also constrains the graph to a small framed area, lets tree-wide expansion actions appear as graph controls, lets long node text and context badges overflow the card, presents category choices as an inline toolbar set that does not scale, scatters blocking and non-blocking messages, and draws dependency edges that merge into solid, unannotated lines at low zoom. Readers cannot tell overlapping dependencies apart or confirm what a line means.

## Solution

Extend the existing read-only Web overview with category selection and a graph presentation alongside the existing tree presentation. Combined remains one hierarchy across categories. Category-grouped mode provides multi-select category buttons with select-all and clear-all actions. When a selected work item needs an ancestor from an unselected category to preserve its actual parent context, the ancestor remains visible as temporary context and is prominently marked as outside the selection.

Add a graph view backed by the same active tree data and relationships. Render parent-child relations as solid edges in a hierarchical layout that defaults to top-to-bottom, with a user-selected left-to-right alternative. When dependency visibility is enabled, render directed dependency relations as dashed edges with arrowheads. The graph is read-only, supports navigation and density controls, reuses the existing item detail, and offers a toggleable structured text/list alternative for keyboard and screen-reader users.

Refine the same overview with a canvas-sized graph, self-contained nodes, presentation-scoped expansion, a compact category control in a shared control cluster, centered blocking states, and legible, identifiable relationship edges. These changes refine the existing tree, filtering, graph, and direction behavior; they add no second view model and no write surface.

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

70. As a project member, I want the graph to fill the content area below the toolbar, so that I can see more of the hierarchy without a small frame.
71. As a project member, I want no decorative frame around the graph, so that the canvas is the space I interact with.
72. As a project member, I want the detail panel to remain a side panel and the graph to refit to the remaining canvas when it opens, so that details and context stay visible together.
73. As a project member, I want unresolved relations in a collapsible bottom strip, so that they are visible without covering graph nodes.
74. As a project member, I want window resizing to preserve my current viewport, so that resizing does not lose my place.
75. As a project member, I want no separate full-screen graph mode, so that the app navigation and global controls stay available.
76. As a project member, I want every node to contain all of its content inside its card, so that text never spills over the graph.
77. As a project member, I want long titles and blocked reasons to wrap within the node, so that I can read them without clipping.
78. As a project member, I want a taller node to grow and the layout to account for its height, so that edges and spacing stay correct.
79. As a project member, I want context badges inside the card, so that the node's content and marking stay together.
80. As a project member, I want no graph text drawn outside a node, so that the drawing stays legible.
81. As a project member, I want a context node to keep the ordinary node layout, so that it reads as a normal item.
82. As a project member, I want a context node distinguished by a dashed outline around the entire card, so that I can spot it.
83. As a project member, I want the context indicator inside the card, so that no label floats outside the node.
84. As a screen-reader user, I want the node's accessible name to state the context, so that the visual badge is not the only cue.
85. As a project member, I want tree Expand all/Collapse all to affect only the tree, so that graph state is not changed from the tree view.
86. As a project member, I want graph Expand all/Collapse all in the graph controls, so that I can manage density without leaving the graph.
87. As a project member, I want the two presentations' expansion states to stay separate, so that optimizing one does not damage the other.
88. As a keyboard user, I want a collapsed graph branch to appear as a "Collapsed — N children" row in the structured list with its own expand control, so that I can expand it without the pointer.
89. As a keyboard user, I want a "Show all" action in the structured list, so that I can restore the full hierarchy quickly.
90. As a project member, I want blocking and empty states centered in the content area, so that I see why there is nothing to browse.
91. As a project member, I want non-blocking notices to stay inline at the top, so that they do not cover the content.
92. As a project member, I want long diagnostics in a width-limited scrollable block, so that they remain readable.
93. As a project member, I want unresolved relations to stay in their own area rather than becoming a centered message, so that content and notices stay distinct.
94. As a project member, I want category selection as a compact "Categories (n)" control, so that many categories do not overwhelm the toolbar.
95. As a project member, I want the control to open a searchable multi-select popover with select-all, clear-all, and a count, so that I can manage a large category set.
96. As a project member, I want the category control, zoom/fit/reset, and direction control in the graph's upper-left control cluster, so that graph controls are together.
97. As a project member, I want the relationship legend at the upper right, so that I can read it without covering the controls.
98. As a tree or structured-list user, I want the same category control in the top toolbar, so that I can filter without a canvas overlay.
99. As a project member, I want the category control hidden in combined mode and keyboard-operable in grouped mode, so that it appears only when it can affect the view and works without a pointer.
100. As a project member, I want dependency edges to stay visibly dashed at every zoom, so that they never read as parent-child lines.
101. As a project member, I want dependency edges to differ from parent-child edges in colour or weight as well as dash, so that the distinction does not rely on one cue.
102. As a project member, I want hovering an edge to raise it and show its relation label, so that I can identify overlapping edges.
103. As a project member, I want coincident parent-child and dependency edges to be offset or haloed, so that both remain visible.
104. As a project member, I want a count or list when several dependencies share a gap, so that I can tell a merged-looking line is multiple relations.
105. As a project member, I want the legend to explain that hovering an edge identifies it, so that I know how to disambiguate.

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

### Page layout and canvas

- The graph fills the content area below the toolbar: no inner fixed-height frame and no decorative border. The right-side detail panel remains; opening it shrinks the graph to the remaining width and refits to that remaining canvas. Unresolved relations sit in a collapsible bottom strip/drawer and never overlay nodes. Window resizing preserves the current viewport and never auto-fits; first graph entry and an explicit direction switch still fit. There is no separate full-screen graph mode.
- Every graph node contains all of its content inside its card: category shape/name, state, title, blocked reason, and any context badge. Text wraps within the node width; a node grows vertically to fit wrapped content; the layout measures rendered node height so edges and spacing account for it. No graph text is drawn outside a node.
- A context node keeps the ordinary node layout and is distinguished by a dashed outline around the entire card, without the hatch or left state stripe. Its context indicator is a text badge inside the card; the node's accessible name states the context. No context label is placed outside the node.
- Expansion actions are presentation-scoped: tree presentation shows the tree's Expand all/Collapse all; graph presentation shows graph-wide Expand all/Collapse all in the graph control area; neither changes the other presentation's state. The structured text alternative mirrors the graph's visible model: a collapsed branch appears as a "Collapsed — N children" row with its own expand control, and the alternative offers "Show all".
- Only blocking and empty states are centered: no category selected, selected categories with no active work, an empty project, a read failure, and a detail-load failure (centered within the detail panel). Non-blocking notices stay inline at the top and are not centered. Long diagnostics use a centered, width-limited, scrollable block. Unresolved relations remain content in their own area.

### Controls and category selection

- Category selection is a compact "Categories (n)" control opening a multi-select popover with search, select-all, clear-all, and the selected count; it is not a set of inline toolbar options. In graph presentation, the category control, zoom/fit/reset, and direction control share a floating control cluster at the canvas upper-left; the relationship legend sits at the upper-right. In tree and structured-list presentations the same compact control stays in the top toolbar and opens the same popover, with no canvas overlay. Combined mode shows no category selector; grouped mode shows it. The popover is keyboard-operable, closes on Escape or an outside click, does not obstruct nodes when closed, and keeps the select-all recovery action in the filtered-empty state.

### Relationship line rendering

- Dependency edges keep a minimum on-screen dash and gap so a single dependency is visibly dashed at every zoom; the dash pattern must not collapse into a solid line at the default fit. Dependency edges use a clearly different colour and/or weight from parent-child edges in addition to the dash; no distinction relies on colour alone.
- Hovering an edge raises and thickens it and shows its relation label, for example "X → Y (depends on)"; the structured text alternative remains the keyboard and screen-reader route to the same relations. Where several dependencies share a visual gap, the interface identifies how many pass through it and lets the reader inspect them; dependencies are not aggregated into one line by default. When a parent-child and a dependency edge coincide on the same pair, the dependency is offset or given a halo so both remain visible. The legend keeps its two entries and adds a hint that hovering an edge identifies it.

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

- Cover the page layout contract: the graph pane has no fixed inner height or decorative frame, the detail panel remains a side panel, unresolved relations are a separate collapsible strip, resizing preserves the viewport, and no full-screen mode exists.
- Cover node content containment: titles, blocked reasons, and context badges render inside the card; long text wraps; a node's measured height is reflected in the layout; no text escapes the node.
- Cover context-node treatment: ordinary layout, dashed full-card outline, inside badge, and an accessible name that states the context.
- Cover presentation-scoped expansion: tree actions affect only the tree, graph actions only the graph, the two states stay separate, and the structured list shows "Collapsed — N children" rows with an expand control plus "Show all".
- Cover message placement: blocking/empty states centered, non-blocking notices inline, long diagnostics in a width-limited scrollable block, unresolved relations in their own area, and detail failures centered within the detail panel.
- Cover the category control: compact "Categories (n)" entry, searchable multi-select popover, select-all/clear-all/count, graph upper-left cluster, legend upper-right, tree toolbar entry, combined-mode absence, keyboard/Escape/outside-close, and the filtered-empty recovery.
- Cover relationship line rendering: the minimum on-screen dash policy, distinct colour/weight, hover raise/label, multi-edge gap identification, coincident pair offset/halo, and the legend hint. Test the dash policy as a pure function and the rendered edge attributes/roles; use a real-browser check for the visual dash and containment cases that jsdom cannot compute.
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

- A separate full-screen graph mode, default aggregation of same-row dependencies, hiding or fading dependency edges by zoom level, inline category chips in the top toolbar, and centering non-blocking notices.

## Further Notes

- The existing directory tree remains the primary overview for users who prefer explicit indentation; the graph is an additional presentation, not a replacement.
- The design map records the settled decisions that led to this specification, including the category context rule, graph edge semantics, hierarchy layout and direction, viewport controls, archive treatment, unresolved handling, accessibility alternative, and legend.
- A small prototype or spike may be used to compare Cytoscape.js and Svelte Flow against the required read-only hierarchical graph, but it must preserve the user-visible contract in this specification and should not expand scope into an editor.
- This specification is ready for implementation planning and ticket decomposition after publication.
- The page optimization decisions extend the same Web surface described above; they are recorded in the design map's Resolved Web page optimization and Verified graph edge behavior sections.
- The direction switch and the page optimization changes are unimplemented in the current checkout; this specification is the buildable delta over the shipped tree, filtering, graph, and preference behavior.
