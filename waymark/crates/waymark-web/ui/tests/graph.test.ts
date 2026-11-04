import { describe, expect, it } from 'vitest';

import { buildGraph, edgeLabel, graphOutline } from '$lib/graph';
import type { OutlineEntry, OutlineItemEntry } from '$lib/graph';
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
			'Dependency: Fix the login form → Login fails (depends on)'
		);
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

	it('carries the server’s blocked reason in prose', () => {
		const outline = graphOutline(buildGraph(hierarchyTree));
		const root = outline.find((entry) => entry.id === 'wm-h-root');
		expect(root).toMatchObject({ blockedReason: 'waiting on wm-h-child (unfinished)' });
		const ready = outline.find((entry) => entry.id === 'wm-h-second-root');
		expect(ready).toMatchObject({ blockedReason: null });
	});
});
