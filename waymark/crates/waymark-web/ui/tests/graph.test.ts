import { describe, expect, it } from 'vitest';

import {
	MAX_GRAPH_ZOOM,
	MIN_FIT_ZOOM,
	MIN_GRAPH_ZOOM,
	buildGraph,
	collapseGraph,
	edgeLabel,
	fittedViewport,
	graphBounds,
	graphNodeLabel,
	graphOutline,
	sameViewport,
	viewportContains,
	type OutlineEntry,
	type OutlineItemEntry
} from '$lib/graph';
import { allIds, filterGroups, visibleRoots } from '$lib/tree';
import { unresolvedReason, type UnresolvedItem } from '$lib/types';
import {
	BUG,
	TASK,
	combinedTree,
	deepGroupedTree,
	dependencyTree,
	groupedDependencyTree,
	groupedTree,
	hierarchyTree,
	unresolvedTree,
	unresolvedReasonsTree
} from './fixtures';

const WITH_DEPENDENCIES = { dependenciesVisible: true } as const;

/** The outline entry for one drawn item, narrowed from the union. */
function drawn(outline: OutlineEntry[], id: string): OutlineItemEntry {
	const entry = outline.find((candidate) => candidate.id === id);
	expect(entry?.kind).toBe('item');
	return entry as OutlineItemEntry;
}

describe('graph model', () => {
	it('places every displayed item exactly once, parents before children', () => {
		const graph = buildGraph(hierarchyTree);
		expect(graph.nodes.map((node) => node.id)).toEqual([
			'wm-h-root',
			'wm-h-child',
			'wm-h-grandchild',
			'wm-h-second-root'
		]);
	});

	it('makes every parentless item a root, with no synthetic root', () => {
		const graph = buildGraph(hierarchyTree);
		expect(graph.roots).toEqual(['wm-h-root', 'wm-h-second-root']);
		expect(graph.nodes.every((node) => node.id !== 'root')).toBe(true);
		expect(graph.nodes.filter((node) => node.parentId === null)).toHaveLength(2);
	});

	it('records the parent and the depth of each item', () => {
		const graph = buildGraph(hierarchyTree);
		const byId = new Map(graph.nodes.map((node) => [node.id, node]));
		expect(byId.get('wm-h-root')?.parentId).toBeNull();
		expect(byId.get('wm-h-root')?.depth).toBe(0);
		expect(byId.get('wm-h-child')?.parentId).toBe('wm-h-root');
		expect(byId.get('wm-h-child')?.depth).toBe(1);
		expect(byId.get('wm-h-grandchild')?.parentId).toBe('wm-h-child');
		expect(byId.get('wm-h-grandchild')?.depth).toBe(2);
	});

	it('draws one solid parent-child edge per relation and nothing else', () => {
		// `wm-task-ship` depends on `wm-bug-login`, but dependency edges are only
		// drawn when the reader asks for them.
		const graph = buildGraph(combinedTree);
		expect(graph.edges).toEqual([
			{
				id: 'wm-bug-login->wm-task-child',
				kind: 'parent-child',
				from: 'wm-bug-login',
				to: 'wm-task-child'
			}
		]);
		expect(graph.dependencies).toEqual([]);
	});

	it('keeps the category shape and the server-derived state cues with each node', () => {
		const graph = buildGraph(combinedTree);
		const blocked = graph.nodes.find((node) => node.id === 'wm-bug-login');
		expect(blocked).toMatchObject({
			title: 'Login fails',
			category: BUG,
			shape: 'circle',
			state: 'blocked',
			blocked: true,
			blockedReason: 'waiting on wm-task-ship (unfinished)'
		});
		const ready = graph.nodes.find((node) => node.id === 'wm-task-ship');
		expect(ready).toMatchObject({ category: TASK, shape: 'square', state: 'ready' });
	});

	it('reads both tree payload shapes from their roots', () => {
		const combined = buildGraph(combinedTree);
		const grouped = buildGraph(groupedTree);
		expect(grouped.nodes.map((node) => node.id)).toEqual(combined.nodes.map((node) => node.id));
		expect(grouped.edges).toEqual(combined.edges);
		expect(grouped.roots).toEqual(combined.roots);
	});

	it('leaves unresolved items out of the graph and lists them separately', () => {
		const graph = buildGraph(unresolvedTree);
		expect(graph.nodes.map((node) => node.id)).toEqual(['wm-bug-login', 'wm-task-child']);
		expect(graph.unresolved.map((item) => item.id)).toEqual(['wm-orphan']);
	});

	it('walks an empty tree without inventing nodes', () => {
		const graph = buildGraph({ view: 'combined', roots: [], unresolved: [], categories: [BUG] });
		expect(graph).toEqual({ nodes: [], edges: [], dependencies: [], roots: [], unresolved: [] });
	});
});

describe('dependency edges in the graph model', () => {
	it('draws none while dependency visibility is off', () => {
		const graph = buildGraph(dependencyTree);
		expect(graph.dependencies).toEqual([]);
		expect(graph.edges.map((edge) => edge.id)).toEqual([
			'wm-dep-bug->wm-dep-fix'
		]);
	});

	it('draws one dashed dependency edge per dependency of a drawn item, pointing at the target', () => {
		const graph = buildGraph(dependencyTree, WITH_DEPENDENCIES);
		expect(graph.dependencies).toEqual([
			{
				id: 'dependency:wm-dep-fix->wm-dep-bug',
				kind: 'dependency',
				from: 'wm-dep-fix',
				to: 'wm-dep-bug',
				toTitle: 'Login fails',
				crossCategory: true
			},
			{
				id: 'dependency:wm-dep-ship->wm-dep-bug',
				kind: 'dependency',
				from: 'wm-dep-ship',
				to: 'wm-dep-bug',
				toTitle: 'Login fails',
				crossCategory: true
			}
		]);
	});

	it('never replaces or hides the parent-child edge between the same two items', () => {
		const graph = buildGraph(dependencyTree, WITH_DEPENDENCIES);
		const pair = (edge: { from: string; to: string }) =>
			`${edge.from}->${edge.to}`;
		// `wm-dep-fix` sits under `wm-dep-bug` and also depends on it, so the pair
		// carries both relations, each in its own direction.
		expect(graph.edges.map(pair)).toContain('wm-dep-bug->wm-dep-fix');
		expect(graph.dependencies.map(pair)).toContain('wm-dep-fix->wm-dep-bug');
		// The solid edge survives unchanged.
		expect(graph.edges).toEqual([
			{
				id: 'wm-dep-bug->wm-dep-fix',
				kind: 'parent-child',
				from: 'wm-dep-bug',
				to: 'wm-dep-fix'
			}
		]);
	});

	it('drops a dependency whose target is not drawn, without a placeholder', () => {
		const graph = buildGraph(dependencyTree, WITH_DEPENDENCIES);
		// `wm-dep-ship` also depends on `wm-dep-archived`, which is absent here.
		expect(graph.dependencies.every((edge) => edge.to !== 'wm-dep-archived')).toBe(true);
		expect(graph.nodes.map((node) => node.id)).not.toContain('wm-dep-archived');
		expect(graph.dependencies).toHaveLength(2);
	});

	it('names both ends of a dependency edge for assistive technology', () => {
		const graph = buildGraph(dependencyTree, WITH_DEPENDENCIES);
		const edge = graph.dependencies[0];
		expect(edgeLabel(graph, edge)).toBe(
			'Dependency: Fix the login form → Login fails (depends on, across categories)'
		);
	});

	it('says when a dependency crosses categories, and only then', () => {
		const graph = buildGraph(dependencyTree, WITH_DEPENDENCIES);
		const cross = graph.dependencies[0];
		const sameCategory = { ...cross, crossCategory: false };
		expect(edgeLabel(graph, sameCategory)).toBe(
			'Dependency: Fix the login form → Login fails (depends on)'
		);
		expect(edgeLabel(graph, cross)).toContain('across categories');
	});
});

describe('collapsing a node', () => {
	it('hides every descendant and keeps the collapsed node itself', () => {
		const graph = buildGraph(hierarchyTree);
		const collapsed = collapseGraph(graph, new Set(['wm-h-child']));
		expect(collapsed.nodes.map((node) => node.id)).toEqual([
			'wm-h-root',
			'wm-h-child',
			'wm-h-second-root'
		]);
		expect(collapsed.nodes.find((node) => node.id === 'wm-h-child')?.collapsed).toBe(true);
		expect(collapsed.nodes.find((node) => node.id === 'wm-h-root')?.collapsed).toBeUndefined();
	});

	it('drops the edges that would point at a hidden node', () => {
		const graph = buildGraph(hierarchyTree);
		const collapsed = collapseGraph(graph, new Set(['wm-h-root']));
		expect(collapsed.edges).toEqual([]);
		expect(collapsed.roots).toEqual(['wm-h-root', 'wm-h-second-root']);
	});

	it('expands back to exactly the original drawing', () => {
		const graph = buildGraph(hierarchyTree);
		expect(collapseGraph(graph, new Set())).toBe(graph);
		const collapsed = collapseGraph(graph, new Set(['wm-h-root']));
		expect(collapseGraph(collapsed, new Set()).nodes.map((node) => node.id)).toEqual([
			'wm-h-root',
			'wm-h-second-root'
		]);
	});

	it('never changes the complete graph or an item', () => {
		const graph = buildGraph(hierarchyTree);
		const before = structuredClone(graph);
		collapseGraph(graph, new Set(['wm-h-child']));
		expect(graph).toEqual(before);
	});

	it('ignores ids the drawing does not contain', () => {
		const graph = buildGraph(hierarchyTree);
		const collapsed = collapseGraph(graph, new Set(['wm-gone', 'wm-h-child']));
		expect(collapsed.nodes.map((node) => node.id)).toEqual([
			'wm-h-root',
			'wm-h-child',
			'wm-h-second-root'
		]);
	});

	it('drops a dependency edge whose end is hidden, and keeps the rest', () => {
		const graph = buildGraph(dependencyTree, WITH_DEPENDENCIES);
		// `wm-dep-fix` is the hidden child; `wm-dep-ship` still depends on the
		// collapsed node itself, which is drawn, so that edge survives.
		const collapsed = collapseGraph(graph, new Set(['wm-dep-bug']));
		expect(collapsed.dependencies.map((edge) => edge.id)).toEqual([
			'dependency:wm-dep-ship->wm-dep-bug'
		]);
	});
});

describe('the fitted viewport', () => {
	const bounds = { x: 16, y: 16, width: 460, height: 276 };

	it('centres the drawing and keeps it inside the canvas', () => {
		const viewport = fittedViewport(bounds, 800, 600);
		expect(viewport.zoom).toBeGreaterThan(MIN_GRAPH_ZOOM);
		expect(viewport.zoom).toBeLessThanOrEqual(MAX_GRAPH_ZOOM);
		// The drawing's centre maps onto the canvas centre.
		expect(viewport.x + (bounds.x + bounds.width / 2) * viewport.zoom).toBeCloseTo(400, 5);
		expect(viewport.y + (bounds.y + bounds.height / 2) * viewport.zoom).toBeCloseTo(300, 5);
		// And every corner is on the canvas.
		expect(viewport.x + bounds.x * viewport.zoom).toBeGreaterThan(0);
		expect(viewport.y + bounds.y * viewport.zoom).toBeGreaterThan(0);
		expect(viewport.x + (bounds.x + bounds.width) * viewport.zoom).toBeLessThan(800);
		expect(viewport.y + (bounds.y + bounds.height) * viewport.zoom).toBeLessThan(600);
	});

	it('clamps a tiny drawing to the maximum zoom', () => {
		expect(fittedViewport({ x: 0, y: 0, width: 10, height: 10 }, 800, 600).zoom).toBe(
			MAX_GRAPH_ZOOM
		);
	});

	it('clamps a huge drawing to the fit floor, and says it is not all visible', () => {
		const huge = { x: 0, y: 0, width: 100000, height: 100000 };
		const viewport = fittedViewport(huge, 800, 600);
		expect(viewport.zoom).toBe(MIN_FIT_ZOOM);
		// Even the widest fit cannot contain an arbitrarily large drawing, and
		// the app must not claim otherwise.
		expect(viewportContains(viewport, huge, 800, 600)).toBe(false);
	});

	it('fits a drawing too tall for the user zoom floor', () => {
		// 1400 px of drawing in a 600 px canvas needs about 0.4: the fit has its
		// own floor, well below the range the user controls stop at.
		const tall = { x: 0, y: 0, width: 800, height: 1400 };
		const viewport = fittedViewport(tall, 800, 600);
		expect(viewport.zoom).toBeLessThan(MIN_GRAPH_ZOOM);
		expect(viewport.y + (tall.y + tall.height) * viewport.zoom).toBeLessThanOrEqual(600);
		expect(viewport.y + tall.y * viewport.zoom).toBeGreaterThanOrEqual(0);
	});

	it('is deterministic, so reset returns to the same viewport', () => {
		expect(fittedViewport(bounds, 800, 600)).toEqual(fittedViewport(bounds, 800, 600));
		expect(
			sameViewport(fittedViewport(bounds, 800, 600), fittedViewport(bounds, 800, 600))
		).toBe(true);
	});

	it('reports containment only when every corner is inside the canvas', () => {
		const fit = fittedViewport(bounds, 800, 600);
		expect(viewportContains(fit, bounds, 800, 600)).toBe(true);
		// Panned far enough, the same drawing no longer fits.
		expect(
			viewportContains({ ...fit, x: fit.x + 400 }, bounds, 800, 600)
		).toBe(false);
		expect(viewportContains(fit, null, 800, 600)).toBe(false);
	});

	it('falls back to the neutral viewport for an empty drawing or an unmeasured canvas', () => {
		expect(fittedViewport(null, 800, 600)).toEqual({ x: 0, y: 0, zoom: 1 });
		expect(fittedViewport(bounds, 0, 0)).toEqual({ x: 0, y: 0, zoom: 1 });
		expect(fittedViewport({ x: 0, y: 0, width: 0, height: 0 }, 800, 600)).toEqual({
			x: 0,
			y: 0,
			zoom: 1
		});
	});

	it('measures the bounds of every drawn node', () => {
		expect(
			graphBounds([
				{ x: 10, y: 20, width: 100, height: 50 },
				{ x: -5, y: 40, width: 20, height: 10 }
			])
		).toEqual({ x: -5, y: 20, width: 115, height: 50 });
		expect(graphBounds([])).toBeNull();
	});
});

describe('the structured text outline', () => {
	it('lists every drawn item once, in drawing order, with its category and state in words', () => {
		const graph = buildGraph(hierarchyTree);
		const outline = graphOutline(graph);
		expect(outline.map((entry) => entry.kind)).toEqual(['item', 'item', 'item', 'item']);
		expect(outline.map((entry) => entry.id)).toEqual(graph.nodes.map((node) => node.id));
		expect(outline[0]).toMatchObject({
			kind: 'item',
			id: 'wm-h-root',
			title: 'Crashes on start',
			category: 'Bugs',
			state: 'Blocked',
			status: 'In progress'
		});
		expect(outline[3]).toMatchObject({ id: 'wm-h-second-root', category: 'Tasks', state: 'Ready' });
	});

	it('states each parent-child relation in words instead of leaving it to indentation', () => {
		const outline = graphOutline(buildGraph(hierarchyTree));
		expect(drawn(outline, 'wm-h-root')).toMatchObject({
			parent: 'Parent: none; this is a top-level item.',
			children: ['Child: Reproduce the crash (wm-h-child).']
		});
		expect(drawn(outline, 'wm-h-child')).toMatchObject({
			parent: 'Parent: Crashes on start (wm-h-root).',
			children: ['Child: Verify the fix (wm-h-grandchild).']
		});
		expect(drawn(outline, 'wm-h-grandchild')).toMatchObject({
			parent: 'Parent: Reproduce the crash (wm-h-child).',
			children: []
		});
	});

	it('leaves dependency relations out while dependency visibility is off', () => {
		const outline = graphOutline(buildGraph(dependencyTree));
		for (const entry of outline) {
			if (entry.kind !== 'item') continue;
			expect(entry.dependencies).toEqual([]);
			expect(entry.dependents).toEqual([]);
		}
		expect(JSON.stringify(outline)).not.toContain('depends on');
	});

	it('states the direction of every dependency relation', () => {
		const outline = graphOutline(buildGraph(dependencyTree, WITH_DEPENDENCIES));
		expect(drawn(outline, 'wm-dep-fix')).toMatchObject({
			dependencies: ['Depends on Login fails (wm-dep-bug).'],
			dependents: []
		});
		expect(drawn(outline, 'wm-dep-bug')).toMatchObject({
			dependencies: [],
			// Both dependents are named, and the sentence says who depends on whom.
			dependents: [
				'Fix the login form (wm-dep-fix) depends on this item.',
				'Ship the release (wm-dep-ship) depends on this item.'
			]
		});
		// The dropped target (`wm-dep-archived`) yields no sentence at all.
		expect(drawn(outline, 'wm-dep-ship').dependencies).toEqual([
			'Depends on Login fails (wm-dep-bug).'
		]);
	});

	it('ends with the unresolved items and the reason each one cannot be placed', () => {
		const outline = graphOutline(buildGraph(unresolvedTree));
		expect(outline.filter((entry) => entry.kind === 'unresolved')).toEqual([
			{
				kind: 'unresolved',
				id: 'wm-orphan',
				title: 'Orphaned work',
				category: 'Tasks',
				reason: 'its parent wm-missing is missing'
			}
		]);
		// Two drawn items (the orphan is not one), then the unresolved entry.
		expect(outline.map((entry) => entry.kind)).toEqual(['item', 'item', 'unresolved']);
	});

	it('words the filter’s context rule, so a kept ancestor is not mistaken for selected work', () => {
		const filtered = filterGroups(deepGroupedTree.groups, new Set(['task']));
		const outline = graphOutline(
			buildGraph(
				{ ...deepGroupedTree, groups: filtered.groups },
				{ contextIds: filtered.contextIds }
			)
		);
		expect(drawn(outline, 'wm-docs-umbrella').contextNote).toBe(
			'Context: outside the category selection (context, outside the selection).'
		);
		expect(drawn(outline, 'wm-bug-mid').context).toBe(true);
		expect(drawn(outline, 'wm-task-leaf')).toMatchObject({
			context: false,
			contextNote: null
		});
	});

	it('drops a collapsed branch from the outline exactly as it drops it from the drawing', () => {
		const graph = buildGraph(hierarchyTree);
		const collapsed = collapseGraph(graph, new Set(['wm-h-child']));
		const outline = graphOutline(collapsed);
		// The collapsed node stays and keeps its own sentence; its child is gone
		// from the outline, so the two readings list the same items.
		expect(outline.map((entry) => entry.id)).toEqual(collapsed.nodes.map((node) => node.id));
		expect(outline.map((entry) => entry.id)).not.toContain('wm-h-grandchild');
		expect(drawn(outline, 'wm-h-child').children).toEqual([]);
		// Nothing else moves: the sibling root and its relation are untouched.
		expect(drawn(outline, 'wm-h-second-root').parent).toBe(
			'Parent: none; this is a top-level item.'
		);
	});

	it('carries the server’s blocked reason in prose', () => {
		const outline = graphOutline(buildGraph(hierarchyTree));
		const root = outline.find((entry) => entry.id === 'wm-h-root');
		expect(root).toMatchObject({ blockedReason: 'waiting on wm-h-child (unfinished)' });
		const ready = outline.find((entry) => entry.id === 'wm-h-second-root');
		expect(ready).toMatchObject({ blockedReason: null });
	});
});

describe('category filtering in the graph model', () => {
	/** The graph Explorer draws for a category scope: the same filtered payload. */
	function scopedGraph(selected: string[]) {
		const filtered = filterGroups(deepGroupedTree.groups, new Set(selected));
		const data = { ...deepGroupedTree, groups: filtered.groups };
		return {
			graph: buildGraph(data, { contextIds: filtered.contextIds }),
			filtered,
			data
		};
	}

	it('shows exactly the items the filtered by-category tree shows, in the same order', () => {
		for (const scope of [['task'], ['bug'], ['bug', 'task'], ['docs'], []]) {
			const { graph, data } = scopedGraph(scope);
			const treeIds = allIds(visibleRoots(data));
			expect(graph.nodes.map((node) => node.id)).toEqual(treeIds);
			expect(graph.roots).toEqual(visibleRoots(data).map((node) => node.id));
		}
	});

	it('marks a required unselected ancestor as context', () => {
		const { graph } = scopedGraph(['task']);
		const byId = new Map(graph.nodes.map((node) => [node.id, node]));
		expect(byId.get('wm-docs-umbrella')?.context).toBe(true);
		expect(byId.get('wm-bug-mid')?.context).toBe(true);
		expect(byId.get('wm-task-leaf')?.context).toBe(false);
	});

	it('does not promote a selected child under an unselected parent to a root', () => {
		const { graph } = scopedGraph(['task']);
		const byId = new Map(graph.nodes.map((node) => [node.id, node]));
		// The task keeps its real parent and depth; the ancestor stays a root.
		expect(byId.get('wm-task-leaf')?.parentId).toBe('wm-bug-mid');
		expect(byId.get('wm-task-leaf')?.depth).toBe(2);
		expect(graph.roots).toEqual(['wm-docs-umbrella', 'wm-task-ship']);
		expect(byId.get('wm-docs-umbrella')?.parentId).toBeNull();
		// No synthetic root is invented for the kept chain.
		expect(graph.nodes.every((node) => node.id !== 'root')).toBe(true);
	});

	it('marks nothing when every category is in scope', () => {
		const { graph, filtered } = scopedGraph(['bug', 'docs', 'task']);
		expect(filtered.contextIds.size).toBe(0);
		expect(graph.nodes.every((node) => node.context === false)).toBe(true);
	});

	it('says context in the accessible name of a context node, with the tree wording', () => {
		const { graph } = scopedGraph(['task']);
		const contextNode = graph.nodes.find((node) => node.id === 'wm-bug-mid');
		const selectedNode = graph.nodes.find((node) => node.id === 'wm-task-leaf');
		expect(contextNode && graphNodeLabel(contextNode)).toContain(
			'context, outside the selection'
		);
		expect(selectedNode && graphNodeLabel(selectedNode)).not.toContain('context');
	});

	it('drops a dependency edge whose target the filtered drawing does not contain', () => {
		// `wm-dep-ship` (Tasks) depends on `wm-dep-bug` (Bugs), which is not an
		// ancestor of it: a task-only scope leaves the dependent drawn and the
		// target out, so the edge goes rather than pointing at nothing.
		const filtered = filterGroups(groupedDependencyTree.groups, new Set(['task']));
		const graph = buildGraph(
			{ ...groupedDependencyTree, groups: filtered.groups },
			{ dependenciesVisible: true, contextIds: filtered.contextIds }
		);
		expect(graph.nodes.map((node) => node.id)).toEqual(['wm-scope-task']);
		// One target was filtered out, the other is archived; both yield no edge.
		expect(graph.dependencies).toEqual([]);
		expect(graph.unresolved).toEqual([]);
	});
});

describe('archive and unresolved entries in the graph model', () => {
	it('never draws an archived item: not a node, a dependency target, or a placeholder', () => {
		const graph = buildGraph(dependencyTree, WITH_DEPENDENCIES);
		expect(graph.nodes.map((node) => node.id)).not.toContain('wm-dep-archived');
		expect(graph.dependencies.flatMap((edge) => [edge.from, edge.to])).not.toContain(
			'wm-dep-archived'
		);
		expect(graph.unresolved.map((item) => item.id)).not.toContain('wm-dep-archived');
		// `wm-dep-ship` still depends on the bug that is drawn.
		expect(graph.dependencies.map((edge) => edge.id)).toContain(
			'dependency:wm-dep-ship->wm-dep-bug'
		);
	});

	it('lists missing-parent and parent-cycle items as unresolved, never as roots', () => {
		const graph = buildGraph(unresolvedReasonsTree);
		expect(graph.unresolved.map((item) => [item.id, item.reason])).toEqual([
			['wm-orphan', 'parent_missing'],
			['wm-cycle-a', 'parent_cycle'],
			['wm-cycle-b', 'parent_cycle']
		]);
		expect(graph.roots).toEqual(['wm-bug-login']);
		const drawn = new Set(graph.nodes.map((node) => node.id));
		for (const id of ['wm-orphan', 'wm-cycle-a', 'wm-cycle-b', 'wm-missing']) {
			expect(drawn.has(id)).toBe(false);
		}
	});

	it('still explains a reason from a vocabulary this build does not know', () => {
		const item = {
			id: 'wm-future',
			title: 'Future relation',
			category: BUG,
			parent: 'wm-gone',
			reason: 'parent_quarantined'
		} as unknown as UnresolvedItem;
		expect(unresolvedReason(item)).toBe('its parent relation is unresolved');
	});
});
