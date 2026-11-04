import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import Explorer from '$lib/components/Explorer.svelte';
import { preferenceKey, projectIdFor } from '$lib/storage';
import { ROOT_A, hierarchyTree, makeFetch } from './fixtures';

/**
 * Presentation-scoped expansion.
 *
 * The tree's Expand all/Collapse all and the graph's graph-wide pair are two
 * different decisions on two different stored sets. These tests read the
 * rendered rows and cards and the project's stored preferences, so they state
 * what a reader sees rather than which component holds which variable.
 */

type FetchOptions = Parameters<typeof makeFetch>[0];

async function renderLoaded(options: FetchOptions) {
	const api = makeFetch(options);
	const view = render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
	await screen.findByTestId('tree-summary');
	return { ...api, ...view };
}

async function openGraph(options: FetchOptions) {
	const rendered = await renderLoaded(options);
	await fireEvent.click(screen.getByLabelText('Graph'));
	await screen.findByTestId('graph');
	await waitFor(() => expect(drawnIds().length).toBeGreaterThan(0));
	return rendered;
}

/** The graph's drawn cards, in document order. */
function drawnIds(): string[] {
	return screen
		.getAllByTestId(/^graph-node-/)
		.map((element) => element.getAttribute('data-node-id') ?? '');
}

function card(id: string): HTMLElement {
	return screen.getByTestId(`graph-node-${id}`);
}

/** The project's stored graph collapse set, or null when none was written. */
function storedCollapsed(): string[] | null {
	const raw = localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'collapsed'));
	return raw === null ? null : (JSON.parse(raw) as string[]);
}

function textOf(element: Element | null): string {
	return element?.textContent?.replace(/\s+/g, ' ').trim() ?? '';
}

describe('presentation-scoped expansion controls', () => {
	it('offers the tree’s Expand all/Collapse all only in tree presentation', async () => {
		await renderLoaded({ trees: { combined: hierarchyTree } });
		expect(screen.getByTestId('tree-expand-all')).not.toBeNull();
		expect(screen.getByTestId('tree-collapse-all')).not.toBeNull();

		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		await waitFor(() => expect(drawnIds().length).toBeGreaterThan(0));

		// The graph presentation does not carry the tree's actions at all.
		expect(screen.queryByTestId('tree-expand-all')).toBeNull();
		expect(screen.queryByTestId('tree-collapse-all')).toBeNull();
		// Its own actions live in the graph's control cluster.
		const cluster = screen.getByTestId('graph-controls-cluster');
		expect(cluster.querySelector('[data-testid="graph-expand-all"]')).not.toBeNull();
		expect(cluster.querySelector('[data-testid="graph-collapse-all"]')).not.toBeNull();
	});

	it('renders the graph actions in the toolbar for the structured list', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		await fireEvent.click(screen.getByLabelText('Structured list'));
		await screen.findByTestId('graph-text');

		expect(screen.queryByTestId('tree-expand-all')).toBeNull();
		expect(screen.queryByTestId('tree-collapse-all')).toBeNull();
		// No canvas to host a floating cluster, so the same graph actions stay
		// in the top toolbar.
		expect(screen.queryByTestId('graph-controls-cluster')).toBeNull();
		expect(screen.getByTestId('graph-expand-all')).not.toBeNull();
		expect(screen.getByTestId('graph-collapse-all')).not.toBeNull();
	});

	it('changes only the tree when the tree collapses everything', async () => {
		await renderLoaded({ trees: { combined: hierarchyTree } });
		await fireEvent.click(screen.getByTestId('tree-collapse-all'));
		expect(screen.queryByTestId('node-wm-h-child')).toBeNull();
		// The tree's own decision is stored on its own key.
		expect(
			localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'expanded', 'combined'))
		).toBe('[]');

		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		await waitFor(() =>
			expect(drawnIds()).toEqual([
				'wm-h-root',
				'wm-h-child',
				'wm-h-grandchild',
				'wm-h-second-root'
			])
		);
		expect(card('wm-h-child').getAttribute('data-collapsed')).toBe('false');
		expect(storedCollapsed()).toBeNull();
	});

	it('changes only the graph when the graph collapses everything', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		await fireEvent.click(screen.getByTestId('graph-collapse-all'));

		// Every drawn node with children folds, so only the roots stay visible.
		await waitFor(() => expect(drawnIds()).toEqual(['wm-h-root', 'wm-h-second-root']));
		expect(card('wm-h-root').getAttribute('data-collapsed')).toBe('true');
		expect(storedCollapsed()).toEqual(['wm-h-root', 'wm-h-child']);
		// The graph's decision never reaches the tree's own expansion key.
		expect(
			localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'expanded', 'combined'))
		).toBeNull();

		await fireEvent.click(screen.getByLabelText('Tree'));
		await screen.findByTestId('node-wm-h-grandchild');
		// The tree was untouched: its rows are still expanded.
		expect(screen.getByTestId('toggle-wm-h-child').getAttribute('aria-expanded')).toBe('true');
	});

	it('clears the stored collapse set when the graph expands everything', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		await fireEvent.click(screen.getByTestId('graph-collapse-all'));
		await waitFor(() => expect(drawnIds()).toEqual(['wm-h-root', 'wm-h-second-root']));

		await fireEvent.click(screen.getByTestId('graph-expand-all'));
		await waitFor(() =>
			expect(drawnIds()).toEqual([
				'wm-h-root',
				'wm-h-child',
				'wm-h-grandchild',
				'wm-h-second-root'
			])
		);
		expect(storedCollapsed()).toEqual([]);
	});

	it('keeps the two expansion states separate across switches', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		await fireEvent.click(screen.getByTestId('graph-toggle-wm-h-child'));
		await waitFor(() => expect(drawnIds()).not.toContain('wm-h-grandchild'));

		// The tree is still fully expanded while the drawing has a fold.
		await fireEvent.click(screen.getByLabelText('Tree'));
		await screen.findByTestId('node-wm-h-grandchild');
		expect(screen.getByTestId('node-wm-h-child')).not.toBeNull();

		// Collapsing the tree changes only the tree's own key.
		await fireEvent.click(screen.getByTestId('tree-collapse-all'));
		expect(screen.queryByTestId('node-wm-h-grandchild')).toBeNull();
		expect(storedCollapsed()).toEqual(['wm-h-child']);

		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		await waitFor(() =>
			expect(drawnIds()).toEqual(['wm-h-root', 'wm-h-child', 'wm-h-second-root'])
		);
		// Only the branch the graph folded is folded; the tree's collapse-all
		// did not fold the other root.
		expect(card('wm-h-root').getAttribute('data-collapsed')).toBe('false');
		expect(card('wm-h-child').getAttribute('data-collapsed')).toBe('true');
	});
});

describe('graph-wide collapse persistence', () => {
	it('stores only the drawn nodes with children, and ignores stale ids', async () => {
		localStorage.setItem(
			preferenceKey(projectIdFor(ROOT_A), 'collapsed'),
			JSON.stringify(['wm-gone'])
		);
		const first = await openGraph({ trees: { combined: hierarchyTree } });
		await fireEvent.click(screen.getByTestId('graph-collapse-all'));
		await waitFor(() => expect(drawnIds()).toEqual(['wm-h-root', 'wm-h-second-root']));

		// The stale id is not carried along: the set describes this drawing.
		expect(storedCollapsed()).toEqual(['wm-h-root', 'wm-h-child']);
		first.unmount();

		const second = await renderLoaded({ trees: { combined: hierarchyTree } });
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		await waitFor(() => expect(drawnIds()).toEqual(['wm-h-root', 'wm-h-second-root']));
		expect(card('wm-h-root').getAttribute('data-collapsed')).toBe('true');
		second.unmount();
	});
});

describe('the structured list mirrors the graph’s visible model', () => {
	/** Fold a branch in the drawing, then read the graph as a structured list. */
	async function openListWithFoldedBranch() {
		const rendered = await openGraph({ trees: { combined: hierarchyTree } });
		await fireEvent.click(screen.getByTestId('graph-toggle-wm-h-child'));
		await waitFor(() => expect(drawnIds()).not.toContain('wm-h-grandchild'));
		await fireEvent.click(screen.getByLabelText('Structured list'));
		await screen.findByTestId('graph-text');
		return rendered;
	}

	it('shows a folded branch as a row with its direct child count and an expand control', async () => {
		await openListWithFoldedBranch();

		const row = screen.getByTestId('graph-text-collapsed-wm-h-child');
		expect(textOf(row)).toContain('Collapsed — 1 children');
		expect(row.textContent).toContain('Collapsed — 1 children');

		const expand = screen.getByTestId('graph-text-expand-wm-h-child');
		expect(expand.tagName).toBe('BUTTON');
		expect(expand.getAttribute('aria-label')).toBe('Expand Reproduce the crash');

		// The hidden child is not listed, so the alternative mirrors the drawing.
		expect(screen.queryByTestId('graph-text-item-wm-h-grandchild')).toBeNull();
	});

	it('expands a folded branch from the list and updates the drawing', async () => {
		await openListWithFoldedBranch();
		await fireEvent.click(screen.getByTestId('graph-text-expand-wm-h-child'));
		await waitFor(() =>
			expect(screen.getByTestId('graph-text-item-wm-h-grandchild')).not.toBeNull()
		);
		// The same graph collapse set was updated.
		expect(storedCollapsed()).toEqual([]);

		await fireEvent.click(screen.getByLabelText('Structured list'));
		await screen.findByTestId('graph');
		await waitFor(() => expect(drawnIds()).toContain('wm-h-grandchild'));
		expect(card('wm-h-child').getAttribute('data-collapsed')).toBe('false');
	});

	it('offers “Show all” to restore the whole hierarchy', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		await fireEvent.click(screen.getByTestId('graph-collapse-all'));
		await waitFor(() => expect(drawnIds()).toEqual(['wm-h-root', 'wm-h-second-root']));
		await fireEvent.click(screen.getByLabelText('Structured list'));
		await screen.findByTestId('graph-text');

		// The folded roots are described, not silently missing their branches.
		expect(screen.getByTestId('graph-text-collapsed-wm-h-root')).not.toBeNull();
		expect(screen.queryByTestId('graph-text-item-wm-h-grandchild')).toBeNull();

		await fireEvent.click(screen.getByTestId('graph-text-show-all'));
		await waitFor(() =>
			expect(screen.getByTestId('graph-text-item-wm-h-grandchild')).not.toBeNull()
		);
		expect(screen.queryByTestId('graph-text-collapsed-wm-h-root')).toBeNull();
		expect(storedCollapsed()).toEqual([]);

		// The drawing reflects the same expansion on the way back.
		await fireEvent.click(screen.getByLabelText('Structured list'));
		await screen.findByTestId('graph');
		await waitFor(() => expect(drawnIds()).toContain('wm-h-grandchild'));
	});
});
