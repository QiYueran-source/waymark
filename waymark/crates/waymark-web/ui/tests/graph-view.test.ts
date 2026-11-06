import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import Explorer from '$lib/components/Explorer.svelte';
import { preferenceKey, projectIdFor } from '$lib/storage';
import {
	ROOT_A,
	ROOT_B,
	combinedTree,
	groupedTree,
	hierarchyTree,
	makeFetch
} from './fixtures';

type FetchOptions = Parameters<typeof makeFetch>[0];

async function renderLoaded(options: FetchOptions) {
	const api = makeFetch(options);
	const view = render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
	// The summary is drawn outside the pane, so it appears whichever
	// presentation the project remembers.
	await screen.findByTestId('tree-summary');
	return { ...api, ...view };
}

async function openGraph(options: FetchOptions) {
	const rendered = await renderLoaded(options);
	await fireEvent.click(screen.getByLabelText('Graph'));
	await screen.findByTestId('graph');
	await waitFor(() => expect(cards().length).toBeGreaterThan(0));
	return rendered;
}

/** The graph's own node cards, in document order. */
function cards(): HTMLElement[] {
	return screen.getAllByTestId(/^graph-node-/);
}

function card(id: string): HTMLElement {
	return screen.getByTestId(`graph-node-${id}`);
}

function number(cardElement: HTMLElement, attribute: string): number {
	const raw = cardElement.getAttribute(attribute);
	expect(raw).not.toBeNull();
	return Number(raw);
}

describe('the presentation control', () => {
	it('starts on the tree, switches to the graph, and back', async () => {
		await renderLoaded({ trees: { combined: combinedTree } });
		expect((screen.getByLabelText('Tree') as HTMLInputElement).checked).toBe(true);
		expect(screen.queryByTestId('graph')).toBeNull();
		expect(screen.getByTestId('node-wm-bug-login')).not.toBeNull();

		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		// The tree is replaced, not stacked under the graph.
		expect(screen.queryByTestId('node-wm-bug-login')).toBeNull();
		expect(screen.getByTestId('graph-node-wm-bug-login')).not.toBeNull();

		await fireEvent.click(screen.getByLabelText('Tree'));
		await screen.findByTestId('node-wm-bug-login');
		expect(screen.queryByTestId('graph')).toBeNull();
	});

	it('reads no extra tree and issues no non-GET request when the presentation changes', async () => {
		const { requests, unmount } = await openGraph({ trees: { combined: combinedTree } });

		expect(requests.every((request) => request.method === 'GET')).toBe(true);
		// The one tree read plus health and the once-per-load status read.
		expect(requests.filter((request) => request.url === '/api/tree?view=combined')).toHaveLength(
			1
		);
		expect(requests.filter((request) => request.url === '/api/status')).toHaveLength(1);
		unmount();
	});

	it('remembers the chosen presentation per project, and not per view', async () => {
		const first = await openGraph({ trees: { combined: combinedTree, grouped: groupedTree } });
		// Switching the data view must not silently switch the presentation back.
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('graph-node-wm-task-ship');
		await fireEvent.click(screen.getByLabelText('Combined'));
		await screen.findByTestId('graph-node-wm-bug-login');
		first.unmount();

		const second = await renderLoaded({ trees: { combined: combinedTree, grouped: groupedTree } });
		await screen.findByTestId('graph');
		expect((screen.getByLabelText('Graph') as HTMLInputElement).checked).toBe(true);
		expect((screen.getByLabelText('Tree') as HTMLInputElement).checked).toBe(false);
		expect(localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'presentation'))).toBe(
			'graph'
		);
		second.unmount();
	});

	it('keeps the presentation out of another project', async () => {
		const first = await openGraph({ root: ROOT_A, trees: { combined: combinedTree } });
		first.unmount();

		const second = makeFetch({ root: ROOT_B, trees: { combined: combinedTree } });
		render(Explorer, { fetchImpl: second.fetchImpl, storage: localStorage });
		await screen.findByTestId('node-wm-bug-login');
		// A fresh project starts on the tree even though A remembers the graph.
		expect((screen.getByLabelText('Tree') as HTMLInputElement).checked).toBe(true);
		expect(screen.queryByTestId('graph')).toBeNull();
	});

	it('falls back to the tree when the stored presentation is unreadable', async () => {
		localStorage.setItem(preferenceKey(projectIdFor(ROOT_A), 'presentation'), 'diagram');
		await renderLoaded({ trees: { combined: combinedTree } });
		expect((screen.getByLabelText('Tree') as HTMLInputElement).checked).toBe(true);
		expect(screen.queryByTestId('graph')).toBeNull();
	});
});

describe('the graph', () => {
	it('draws every active item exactly once', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		const ids = cards().map((element) => element.getAttribute('data-node-id'));
		expect(ids).toEqual(['wm-h-root', 'wm-h-child', 'wm-h-grandchild', 'wm-h-second-root']);
		for (const id of ids) {
			expect(screen.getAllByTestId(`graph-node-${id}`)).toHaveLength(1);
		}
	});

	it('makes every parentless item a root and invents none', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		const roots = cards().filter((element) => element.getAttribute('data-depth') === '0');
		expect(roots.map((element) => element.getAttribute('data-node-id'))).toEqual([
			'wm-h-root',
			'wm-h-second-root'
		]);
		for (const root of roots) {
			expect(root.getAttribute('data-parent-id')).toBe('');
		}
		// There is no box labelled "root" wrapping the real roots.
		expect(screen.queryByTestId('graph-node-root')).toBeNull();
	});

	it('places each item below its parent, one row per generation', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		expect(number(card('wm-h-child'), 'data-graph-y')).toBeGreaterThan(
			number(card('wm-h-root'), 'data-graph-y')
		);
		expect(number(card('wm-h-grandchild'), 'data-graph-y')).toBeGreaterThan(
			number(card('wm-h-child'), 'data-graph-y')
		);
		// The roots share the top row; generation 2 sits below generation 1.
		expect(number(card('wm-h-second-root'), 'data-graph-y')).toBe(
			number(card('wm-h-root'), 'data-graph-y')
		);
		expect(number(card('wm-h-child'), 'data-depth')).toBe(
			number(card('wm-h-root'), 'data-depth') + 1
		);
		expect(number(card('wm-h-grandchild'), 'data-depth')).toBe(2);
		expect(number(card('wm-h-grandchild'), 'data-graph-y')).toBeGreaterThan(
			number(card('wm-h-second-root'), 'data-graph-y')
		);
	});

	it('renders one solid parent-child edge per parent-child relation', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		const edges = screen.getAllByTestId('graph-edge');
		expect(
			edges.map((edge) => [edge.getAttribute('data-from'), edge.getAttribute('data-to')])
		).toEqual([
			['wm-h-root', 'wm-h-child'],
			['wm-h-child', 'wm-h-grandchild']
		]);
		for (const edge of edges) {
			expect(edge.getAttribute('data-edge-kind')).toBe('parent-child');
			expect(edge.getAttribute('aria-label')).toMatch(/^Parent-child: .+ → .+$/);
			// Solid, i.e. no dash pattern on its path.
			for (const path of edge.querySelectorAll('path')) {
				expect(path.getAttribute('stroke-dasharray')).toBeNull();
				expect(path.getAttribute('style') ?? '').not.toContain('dasharray');
			}
		}
	});

	it('shows the category shape and name, the state, and the blocked marker', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		const root = card('wm-h-root');

		// Category: Bugs are circles; the name is written out beside the shape.
		const shape = root.querySelector('.shape');
		expect(shape?.getAttribute('data-shape')).toBe('circle');
		expect(shape?.getAttribute('aria-label')).toBe('Bugs category');
		expect(root.textContent).toContain('Bugs');

		// State: the word and the existing state class, not colour alone.
		const state = screen.getByTestId('graph-state-wm-h-root');
		expect(state.textContent).toBe('Blocked');
		expect(state.className).toContain('state-blocked');

		// Blocked: the marker and the server's reason.
		expect(root.querySelector('.blocked-marker')?.textContent).toBe('!');
		expect(screen.getByTestId('graph-blocked-reason-wm-h-root').textContent).toContain(
			'waiting on wm-h-child (unfinished)'
		);

		// A different category keeps its own shape; Tasks are squares.
		expect(card('wm-h-grandchild').querySelector('.shape')?.getAttribute('data-shape')).toBe(
			'square'
		);
		expect(screen.getByTestId('graph-state-wm-h-grandchild').textContent).toBe('Ready');
	});

	it('explains the solid edge in a legend', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		const legend = screen.getByTestId('graph-legend');
		expect(legend.textContent).toMatch(/Solid line/);
		expect(legend.textContent).toMatch(/parent-child/i);
	});

	it('draws the dependency overlay only when dependency visibility is on', async () => {
		const rendered = await renderLoaded({ trees: { combined: combinedTree } });
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');

		// The fixture has wm-task-ship depending on wm-bug-login: with the
		// overlay on, the hierarchy edge keeps its place and the dependency is a
		// second, dashed, directed edge. `graph-dependency.test.ts` owns the
		// detailed overlay contract.
		const edges = screen.getAllByTestId('graph-edge');
		expect(edges).toHaveLength(2);
		const solid = edges.find((edge) => edge.getAttribute('data-edge-kind') === 'parent-child');
		expect(solid?.getAttribute('data-from')).toBe('wm-bug-login');
		expect(solid?.getAttribute('data-to')).toBe('wm-task-child');
		const dashed = edges.find((edge) => edge.getAttribute('data-edge-kind') === 'dependency');
		expect(dashed?.getAttribute('data-from')).toBe('wm-task-ship');
		expect(dashed?.getAttribute('data-to')).toBe('wm-bug-login');
		expect(rendered.requests.every((request) => request.method === 'GET')).toBe(true);
	});

	it('does not move a node when it is dragged', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		const element = card('wm-h-root');
		const wrapper = element.closest('.svelte-flow__node') as HTMLElement;
		expect(wrapper).not.toBeNull();
		expect(wrapper.classList.contains('draggable')).toBe(false);
		const before = wrapper.getAttribute('style');

		await fireEvent.pointerDown(element, { button: 0, clientX: 20, clientY: 20 });
		await fireEvent.pointerMove(element, { clientX: 120, clientY: 160 });
		await fireEvent.pointerUp(element, { clientX: 120, clientY: 160 });

		expect(wrapper.getAttribute('style')).toBe(before);
		// The layout the app handed the renderer is untouched too.
		expect(number(card('wm-h-root'), 'data-graph-y')).toBe(
			number(card('wm-h-second-root'), 'data-graph-y')
		);
	});

	it('lists items whose parent relation is unresolved beside the graph', async () => {
		const orphans = {
			view: 'combined' as const,
			roots: hierarchyTree.roots,
			categories: hierarchyTree.categories,
			unresolved: [
				{
					id: 'wm-h-orphan',
					title: 'Adopt the orphan',
					category: hierarchyTree.categories[0],
					parent: 'wm-missing',
					reason: 'parent_missing' as const
				}
			]
		};
		await openGraph({ trees: { combined: orphans } });
		expect(screen.getByTestId('unresolved').textContent).toContain('Adopt the orphan');
		expect(screen.queryByTestId('graph-node-wm-h-orphan')).toBeNull();
	});

	it('renders the grouped payload as one hierarchy, once per item', async () => {
		await openGraph({ trees: { combined: combinedTree, grouped: groupedTree } });
		await fireEvent.click(screen.getByLabelText('By category'));
		await waitFor(() => expect(cards()).toHaveLength(3));
		expect(screen.getAllByTestId('graph-node-wm-bug-login')).toHaveLength(1);
		expect(
			screen
				.getAllByTestId('graph-edge')
				.filter((edge) => edge.getAttribute('data-edge-kind') === 'dependency')
		).toHaveLength(0);
	});
});
