import { describe, expect, it } from 'vitest';

import {
	MAX_GRAPH_ZOOM,
	MIN_GRAPH_ZOOM,
	buildGraph,
	collapseGraph,
	edgeLabel,
	fittedViewport,
	graphBounds,
	sameViewport
} from '$lib/graph';
import {
	BUG,
	TASK,
	combinedTree,
	dependencyTree,
	groupedTree,
	hierarchyTree,
	unresolvedTree
} from './fixtures';

const WITH_DEPENDENCIES = { dependenciesVisible: true } as const;

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
			'Dependency: Fix the login form → Login fails (depends on)'
		);
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

	it('clamps a huge drawing to the minimum zoom', () => {
		expect(fittedViewport({ x: 0, y: 0, width: 100000, height: 100000 }, 800, 600).zoom).toBe(
			MIN_GRAPH_ZOOM
		);
	});

	it('is deterministic, so reset returns to the same viewport', () => {
		expect(fittedViewport(bounds, 800, 600)).toEqual(fittedViewport(bounds, 800, 600));
		expect(
			sameViewport(fittedViewport(bounds, 800, 600), fittedViewport(bounds, 800, 600))
		).toBe(true);
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
