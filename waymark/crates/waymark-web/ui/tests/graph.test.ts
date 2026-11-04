import { describe, expect, it } from 'vitest';

import { buildGraph } from '$lib/graph';
import { BUG, TASK, combinedTree, groupedTree, hierarchyTree, unresolvedTree } from './fixtures';

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
		// `wm-task-ship` depends on `wm-bug-login`, but dependencies are not part
		// of the hierarchy drawing.
		const graph = buildGraph(combinedTree);
		expect(graph.edges).toEqual([
			{ id: 'wm-bug-login->wm-task-child', from: 'wm-bug-login', to: 'wm-task-child' }
		]);
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
		expect(graph).toEqual({ nodes: [], edges: [], roots: [], unresolved: [] });
	});
});
