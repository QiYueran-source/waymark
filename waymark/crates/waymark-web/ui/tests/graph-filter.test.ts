import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import Explorer from '$lib/components/Explorer.svelte';
import { failure } from './fixtures';
import {
	combinedTree,
	deepGroupedTree,
	dependencyTree,
	groupedDependencyTree,
	groupedTreeWithEmptyCategory,
	makeFetch,
	unresolvedReasonsTree
} from './fixtures';
import {
	categoryOption,
	clickCategory,
	clickCategoryAction,
	openCategories
} from './category-helpers';

type FetchOptions = Parameters<typeof makeFetch>[0];

async function renderLoaded(options: FetchOptions) {
	const api = makeFetch(options);
	const view = render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
	await screen.findByTestId('tree-summary');
	return { ...api, ...view };
}

/** Render, switch to the by-category view, then to the graph presentation. */
async function openGroupedGraph(options: FetchOptions) {
	const rendered = await renderLoaded(options);
	await fireEvent.click(screen.getByLabelText('By category'));
	await screen.findByTestId('tree-summary');
	await fireEvent.click(screen.getByLabelText('Graph'));
	await screen.findByTestId('graph');
	return rendered;
}

/** The graph's own node cards, in document order. */
function cards(): HTMLElement[] {
	return screen.queryAllByTestId(/^graph-node-/);
}

function card(id: string): HTMLElement {
	return screen.getByTestId(`graph-node-${id}`);
}

function textOf(element: Element | null): string {
	return element?.textContent?.replace(/\s+/g, ' ').trim() ?? '';
}

/** Unselect every category from a fully selected start. */
async function clearCategories(keys: string[]): Promise<void> {
	for (const key of keys) {
		await clickCategory(key);
	}
}

const DEEP = { combined: combinedTree, grouped: deepGroupedTree };
const DEEP_CATEGORIES = ['bug', 'docs', 'task'];

describe('the category filter governs the graph', () => {
	it('filters the graph exactly as the by-category tree for the same selection', async () => {
		await openGroupedGraph({ trees: DEEP });

		// Task-only: the tree keeps the whole unselected chain down to the task,
		// and the graph must draw the same items in the same order.
		await clickCategory('bug');
		await clickCategory('docs');
		await waitFor(() =>
			expect(cards().map((element) => element.getAttribute('data-node-id'))).toEqual([
				'wm-docs-umbrella',
				'wm-bug-mid',
				'wm-task-leaf',
				'wm-task-ship'
			])
		);

		// The same scope in the tree presentation shows exactly those rows.
		await fireEvent.click(screen.getByLabelText('Tree'));
		await screen.findByTestId('node-wm-task-leaf');
		const treeIds = screen
			.getAllByTestId(/^node-/)
			.map((element) => (element.getAttribute('data-testid') ?? '').slice('node-'.length));
		expect(treeIds).toEqual([
			'wm-docs-umbrella',
			'wm-bug-mid',
			'wm-task-leaf',
			'wm-task-ship'
		]);

		// Back to the graph: still identical, so switching presentation never
		// changes what the filter means.
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		expect(cards().map((element) => element.getAttribute('data-node-id'))).toEqual(treeIds);
	});

	it('keeps the same multi-select control, with select-all and clear-all', async () => {
		await openGroupedGraph({ trees: DEEP });

		expect(screen.getByTestId('category-filter')).not.toBeNull();
		expect((await categoryOption('bug')).getAttribute('aria-pressed')).toBe('true');

		await clickCategory('bug');
		// The bug leaves the selection but stays as the task's required ancestor,
		// now marked context; its unrelated sibling leaves entirely.
		await waitFor(() => expect(card('wm-bug-mid').getAttribute('data-context')).toBe('true'));
		expect(screen.queryByTestId('graph-node-wm-bug-sibling')).toBeNull();
		expect((await categoryOption('bug')).getAttribute('aria-pressed')).toBe('false');

		await clickCategoryAction('Clear all categories');
		expect(cards()).toHaveLength(0);
		// The popover's select-all and the pane's recovery action reach the same
		// state; the control is available again in the toolbar.
		await clickCategoryAction('Select all categories');
		await waitFor(() => expect(cards()).toHaveLength(6));
	});

	it('draws the required unselected ancestors as marked context', async () => {
		await openGroupedGraph({ trees: DEEP });
		await clickCategory('bug');
		await clickCategory('docs');

		await waitFor(() =>
			expect(card('wm-docs-umbrella').getAttribute('data-context')).toBe('true')
		);
		expect(card('wm-bug-mid').getAttribute('data-context')).toBe('true');

		// The marking is visible text with the tree's own wording, hidden from
		// assistive technology because the title's name already says it.
		const marker = screen.getByTestId('graph-context-marker-wm-bug-mid');
		expect(marker.getAttribute('aria-hidden')).toBe('true');
		expect(textOf(marker)).toBe('Context — outside the selection');
		expect(screen.getByTestId('graph-select-wm-bug-mid').getAttribute('aria-label')).toBe(
			'Middle bug — Bugs, Blocked (context, outside the selection)'
		);

		// Selected work carries neither cue.
		expect(card('wm-task-leaf').getAttribute('data-context')).toBeNull();
		expect(screen.queryByTestId('graph-context-marker-wm-task-leaf')).toBeNull();
		expect(screen.getByTestId('graph-select-wm-task-leaf').getAttribute('aria-label')).toBeNull();
	});

	it('never promotes a selected item under an unselected parent to a root', async () => {
		await openGroupedGraph({ trees: DEEP });
		await clickCategory('bug');
		await clickCategory('docs');
		await waitFor(() => expect(cards()).toHaveLength(4));

		expect(card('wm-task-leaf').getAttribute('data-parent-id')).toBe('wm-bug-mid');
		expect(card('wm-task-leaf').getAttribute('data-depth')).toBe('2');
		const roots = cards().filter((element) => element.getAttribute('data-depth') === '0');
		expect(roots.map((element) => element.getAttribute('data-node-id'))).toEqual([
			'wm-docs-umbrella',
			'wm-task-ship'
		]);
		// The context ancestor stays where the hierarchy put it, under no one.
		expect(card('wm-docs-umbrella').getAttribute('data-parent-id')).toBe('');
		// And no invented wrapper node appears for the kept chain.
		expect(screen.queryByTestId('graph-node-root')).toBeNull();
	});

	it('marks no node when every category is in scope', async () => {
		await openGroupedGraph({ trees: DEEP });
		expect(cards()).toHaveLength(6);
		expect(screen.queryAllByTestId(/^graph-context-marker-/)).toHaveLength(0);
		for (const element of cards()) {
			expect(element.getAttribute('data-context')).toBeNull();
		}
	});

	it('counts only selected work in the summary, not context rows', async () => {
		await openGroupedGraph({ trees: DEEP });
		await clickCategory('bug');
		await clickCategory('docs');
		// Two tasks are selected work; the blocked context ancestors are not.
		expect(screen.getByTestId('tree-summary').textContent).toBe('2 items · 0 blocked');
	});

	it('leaves the combined graph unfiltered, as the combined tree is', async () => {
		await renderLoaded({ trees: { combined: combinedTree, grouped: deepGroupedTree } });
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		expect(screen.queryByTestId('category-filter')).toBeNull();
		expect(cards()).toHaveLength(3);
		expect(screen.queryAllByTestId(/^graph-context-marker-/)).toHaveLength(0);
	});
});

describe('empty states in the graph pane', () => {
	it('shows an explicit empty state after clearing every category, with select-all', async () => {
		await openGroupedGraph({ trees: DEEP });
		await clearCategories(DEEP_CATEGORIES);

		const empty = await screen.findByTestId('filtered-empty-state');
		expect(empty.getAttribute('data-empty-kind')).toBe('categories-cleared');
		expect(textOf(empty.querySelector('h2'))).toBe('No categories selected');
		expect(empty.textContent).toContain('graph');
		expect(screen.queryByTestId('graph')).toBeNull();

		await clickCategoryAction('Select all categories');
		await screen.findByTestId('graph');
		await waitFor(() => expect(cards()).toHaveLength(6));
		expect(screen.queryByTestId('filtered-empty-state')).toBeNull();
	});

	it('shows the empty state when the selected categories hold no active work', async () => {
		// Docs is configured but holds no active work in this read.
		await openGroupedGraph({ trees: { combined: combinedTree, grouped: groupedTreeWithEmptyCategory } });
		await clickCategory('bug');
		await clickCategory('task');

		const empty = await screen.findByTestId('filtered-empty-state');
		expect(empty.getAttribute('data-empty-kind')).toBe('no-active-work');
		expect(textOf(empty.querySelector('h2'))).toBe('Nothing in the selected categories');
		expect(screen.queryByTestId('graph')).toBeNull();

		await clickCategoryAction('Select all categories');
		await waitFor(() => expect(cards().length).toBeGreaterThan(0));
		expect((await categoryOption('docs')).getAttribute('aria-pressed')).toBe('true');
	});

	it('keeps the toolbar recovery action and the pane one in agreement', async () => {
		await openGroupedGraph({ trees: DEEP });
		await clearCategories(DEEP_CATEGORIES);
		const empty = await screen.findByTestId('filtered-empty-state');
		// The toolbar's control and the pane's recovery action say the same thing.
		await openCategories();
		const recovery = screen.getAllByRole('button', { name: 'Select all categories' });
		expect(recovery).toHaveLength(2);
		expect(within(empty).getByTestId('select-all-from-empty')).not.toBeNull();
		expect(recovery.some((button) => empty.contains(button))).toBe(true);
	});
});

describe('archive, unresolved, and damaged reads in the graph', () => {
	it('draws no archived item as a node, a target, or a placeholder', async () => {
		await renderLoaded({ trees: { combined: dependencyTree } });
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		await waitFor(() =>
			expect(
				screen
					.getAllByTestId('graph-edge')
					.filter((edge) => edge.getAttribute('data-edge-kind') === 'dependency')
			).toHaveLength(2)
		);

		// `wm-dep-ship` depends on `wm-dep-archived`, which the server excluded
		// from the payload: the graph neither draws it nor dangles an edge at it.
		expect(screen.queryByTestId('graph-node-wm-dep-archived')).toBeNull();
		expect(
			screen
				.getAllByTestId('graph-edge')
				.some((edge) => edge.getAttribute('data-to') === 'wm-dep-archived')
		).toBe(false);
		expect(screen.queryByTestId('unresolved')).toBeNull();
	});

	it('drops a dependency target that filtering leaves out', async () => {
		await openGroupedGraph({ trees: { combined: combinedTree, grouped: groupedDependencyTree } });
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		await clickCategory('bug');

		await waitFor(() =>
			expect(cards().map((element) => element.getAttribute('data-node-id'))).toEqual([
				'wm-scope-task'
			])
		);
		/** Every drawn edge, hierarchy and overlay alike. */
		const drawn = screen.queryAllByTestId('graph-edge');
		expect(drawn.filter((edge) => edge.getAttribute('data-edge-kind') === 'dependency')).toEqual(
			[]
		);
	});

	it('lists missing-parent and parent-cycle items with their reason, never as roots', async () => {
		await renderLoaded({ trees: { combined: unresolvedReasonsTree } });
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');

		const unresolved = screen.getByTestId('unresolved');
		const items = within(unresolved).getAllByTestId('unresolved-item');
		expect(
			items.map((item) => [item.getAttribute('data-reason'), textOf(item)])
		).toEqual([
			['parent_missing', 'Orphaned work wm-orphan its parent wm-missing is missing'],
			['parent_cycle', 'Cycle member A wm-cycle-a its parent wm-cycle-b forms a cycle'],
			['parent_cycle', 'Cycle member B wm-cycle-b its parent wm-cycle-a forms a cycle']
		]);
		for (const item of items) {
			expect(textOf(item)).toMatch(/is missing|forms a cycle/);
		}

		// Not roots, not nodes, and no invented parent for any of them.
		const ids = cards().map((element) => element.getAttribute('data-node-id'));
		expect(ids).toEqual(['wm-bug-login', 'wm-task-child']);
		for (const id of ['wm-orphan', 'wm-cycle-a', 'wm-cycle-b', 'wm-missing']) {
			expect(screen.queryByTestId(`graph-node-${id}`)).toBeNull();
		}
		expect(screen.queryByTestId('graph-node-root')).toBeNull();
	});

	it('renders the existing read failure instead of a partial graph', async () => {
		const api = makeFetch({
			treeFailure: failure('INCOMPLETE_PROJECTION', 'the project projection is incomplete', [
				'work/tasks/wm-broken.md: damaged document'
			])
		});
		// A remembered graph presentation must not change the failure handling.
		localStorage.setItem('waymark:project:/projects/alpha:presentation', 'graph');
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });

		const readFailure = await screen.findByTestId('read-failure');
		expect(readFailure.getAttribute('role')).toBe('alert');
		expect(screen.getByTestId('read-failure-code').textContent).toContain(
			'INCOMPLETE_PROJECTION'
		);
		expect(screen.queryByTestId('graph')).toBeNull();
		expect(screen.queryByTestId('filtered-empty-state')).toBeNull();
		expect(screen.queryByTestId('empty-state')).toBeNull();
		expect(screen.queryAllByTestId(/^graph-node-/)).toHaveLength(0);
	});
});
