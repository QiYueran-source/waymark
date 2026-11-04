import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import Explorer from '$lib/components/Explorer.svelte';
import {
	combinedTree,
	groupedTreeWithEmptyCategory,
	makeFetch,
	type FetchOptions
} from './fixtures';
import { clickCategory, clickCategoryAction, openCategories } from './category-helpers';

async function renderLoaded(options: FetchOptions) {
	const api = makeFetch(options);
	const view = render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
	await screen.findByTestId('tree-summary');
	return { ...api, ...view };
}

/** Render, then switch to the by-category view and wait for its first group. */
async function renderGrouped(options: FetchOptions = { trees: BOTH_VIEWS }) {
	const rendered = await renderLoaded(options);
	await fireEvent.click(screen.getByLabelText('By category'));
	await screen.findByTestId('group-bug');
	return rendered;
}

/** Render, switch to the by-category view, then to the graph presentation. */
async function openGroupedGraph(options: FetchOptions = { trees: BOTH_VIEWS }) {
	const rendered = await renderGrouped(options);
	await fireEvent.click(screen.getByLabelText('Graph'));
	await screen.findByTestId('graph');
	await waitFor(() => expect(screen.getAllByTestId(/^graph-node-/).length).toBeGreaterThan(0));
	return rendered;
}

const BOTH_VIEWS = { combined: combinedTree, grouped: groupedTreeWithEmptyCategory };

function trigger(): HTMLElement {
	return screen.getByTestId('category-trigger');
}

function triggerText(): string {
	return trigger().textContent?.replace(/\s+/g, ' ').trim() ?? '';
}

function textOf(element: Element | null): string {
	return element?.textContent?.replace(/\s+/g, ' ').trim() ?? '';
}

describe('the compact category control', () => {
	it('is one closed control that states the selected count', async () => {
		await renderGrouped();

		expect(triggerText()).toBe('Categories (3)');
		expect(trigger().getAttribute('aria-expanded')).toBe('false');
		// Closed means gone, not merely off-screen: no options or actions exist
		// for assistive technology to wander into.
		expect(screen.queryByTestId('category-popover')).toBeNull();
		expect(screen.queryAllByTestId(/^category-option-/)).toHaveLength(0);
		expect(screen.queryByRole('button', { name: 'Select all categories' })).toBeNull();
		expect(screen.queryByRole('button', { name: 'Clear all categories' })).toBeNull();
	});

	it('opens a multi-select popover with every configured category', async () => {
		await renderGrouped();
		const popover = await openCategories();

		expect(trigger().getAttribute('aria-expanded')).toBe('true');
		expect(popover.getAttribute('role')).toBe('dialog');
		expect(popover.getAttribute('aria-label')).toBe('Categories');
		// Options keep their shape, name, and the selected state in words.
		expect(
			Array.from(popover.querySelectorAll('[data-testid^="category-option-"]')).map((option) =>
				option.getAttribute('data-testid')
			)
		).toEqual(['category-option-bug', 'category-option-docs', 'category-option-task']);
		const docs = within(popover).getByTestId('category-option-docs');
		expect(docs.querySelector('[data-shape]')?.getAttribute('data-shape')).toBe('triangle');
		expect(docs.getAttribute('aria-label')).toBe('Docs, selected');
		expect(docs.getAttribute('aria-pressed')).toBe('true');
	});

	it('updates the count and the option state on selection, clear-all, and select-all', async () => {
		await renderGrouped();
		await openCategories();

		await clickCategory('docs');
		expect(triggerText()).toBe('Categories (2)');
		expect(screen.getByTestId('category-option-docs').getAttribute('aria-pressed')).toBe('false');
		expect(screen.getByTestId('category-option-docs').getAttribute('aria-label')).toBe(
			'Docs, not selected'
		);

		await clickCategoryAction('Clear all categories');
		expect(triggerText()).toBe('Categories (0)');
		for (const key of ['bug', 'docs', 'task']) {
			expect(screen.getByTestId(`category-option-${key}`).getAttribute('aria-pressed')).toBe(
				'false'
			);
		}

		await clickCategoryAction('Select all categories');
		expect(triggerText()).toBe('Categories (3)');
		for (const key of ['bug', 'docs', 'task']) {
			expect(screen.getByTestId(`category-option-${key}`).getAttribute('aria-pressed')).toBe(
				'true'
			);
		}
	});

	it('filters the options case-insensitively and says when nothing matches', async () => {
		await renderGrouped();
		await openCategories();

		const search = screen.getByTestId('category-search');
		expect(search.getAttribute('aria-label')).toBe('Search categories');

		await fireEvent.input(search, { target: { value: 'BUG' } });
		expect(screen.getByTestId('category-option-bug')).not.toBeNull();
		expect(screen.queryByTestId('category-option-docs')).toBeNull();
		expect(screen.queryByTestId('category-option-task')).toBeNull();

		await fireEvent.input(search, { target: { value: 'zzz' } });
		expect(screen.queryAllByTestId(/^category-option-/)).toHaveLength(0);
		const noMatch = screen.getByTestId('category-no-match');
		expect(textOf(noMatch)).toContain('No categories match');
		// The recovery action stays available in the filtered-empty state.
		expect(screen.getByRole('button', { name: 'Select all categories' })).not.toBeNull();
	});

	it('clears the search when it closes, so it reopens whole', async () => {
		await renderGrouped();
		await openCategories();
		await fireEvent.input(screen.getByTestId('category-search'), { target: { value: 'task' } });
		expect(screen.queryByTestId('category-option-bug')).toBeNull();

		await fireEvent.keyDown(screen.getByTestId('category-search'), { key: 'Escape' });
		expect(screen.queryByTestId('category-popover')).toBeNull();

		await openCategories();
		expect(screen.getByTestId('category-option-bug')).not.toBeNull();
		expect((screen.getByTestId('category-search') as HTMLInputElement).value).toBe('');
	});

	it('closes on Escape and returns focus to the trigger', async () => {
		await renderGrouped();
		trigger().focus();
		await openCategories();
		// The search is where a keyboard reader starts.
		expect(document.activeElement).toBe(screen.getByTestId('category-search'));

		await fireEvent.keyDown(screen.getByTestId('category-search'), { key: 'Escape' });
		expect(screen.queryByTestId('category-popover')).toBeNull();
		expect(document.activeElement).toBe(trigger());

		// Closing from the trigger returns focus there too.
		await fireEvent.click(trigger());
		await waitFor(() => expect(document.activeElement).toBe(screen.getByTestId('category-search')));
		await fireEvent.click(trigger());
		await waitFor(() => expect(document.activeElement).toBe(trigger()));
	});

	it('closes on an outside click but stays open for a click inside', async () => {
		await renderGrouped();
		const popover = await openCategories();

		await fireEvent.pointerDown(popover);
		expect(screen.queryByTestId('category-popover')).not.toBeNull();

		await fireEvent.pointerDown(document.body);
		expect(screen.queryByTestId('category-popover')).toBeNull();
	});

	it('is operable with the keyboard: native buttons, and Enter/Space activate them', async () => {
		await renderGrouped();
		expect(trigger().tagName).toBe('BUTTON');
		await openCategories();
		const option = screen.getByTestId('category-option-task');
		expect(option.tagName).toBe('BUTTON');
		// A native button responds to Enter and Space as a click; the test's
		// click stands in for that keyboard activation and must toggle the option.
		await fireEvent.click(option);
		expect(option.getAttribute('aria-pressed')).toBe('false');
		expect(triggerText()).toBe('Categories (2)');
	});

	it('keeps the stable test ids and action names the other suites use', async () => {
		await renderGrouped();
		expect(screen.getByTestId('category-filter')).not.toBeNull();
		await openCategories();
		expect(screen.getByTestId('category-search')).not.toBeNull();
		expect(screen.getByRole('button', { name: 'Select all categories' })).not.toBeNull();
		expect(screen.getByRole('button', { name: 'Clear all categories' })).not.toBeNull();
	});
});

describe('where the control lives', () => {
	it('sits in the top toolbar in tree presentation, with no canvas overlay', async () => {
		await renderGrouped();
		const filter = screen.getByTestId('category-filter');
		expect(filter.closest('.toolbar')).not.toBeNull();
		expect(filter.closest('.svelte-flow__panel')).toBeNull();
		expect(screen.queryByTestId('graph-controls')).toBeNull();
	});

	it('sits in the upper-left graph control cluster in graph presentation', async () => {
		await openGroupedGraph();
		const filter = screen.getByTestId('category-filter');
		const controls = screen.getByTestId('graph-controls');
		expect(controls.contains(filter)).toBe(true);
		// Closed, it adds one compact trigger and no popover to the canvas.
		expect(screen.queryByTestId('category-popover')).toBeNull();
		expect(trigger().getAttribute('aria-expanded')).toBe('false');
		// The cluster is the canvas's upper-left panel, with zoom/fit/reset.
		const panel = controls.closest('.svelte-flow__panel');
		expect(panel?.classList.contains('top')).toBe(true);
		expect(panel?.classList.contains('left')).toBe(true);
		for (const id of ['graph-zoom-out', 'graph-zoom-in', 'graph-fit', 'graph-reset']) {
			expect(within(controls).getByTestId(id)).not.toBeNull();
		}
		// The toolbar carries no second copy.
		expect(screen.getByTestId('category-filter').closest('.toolbar')).toBeNull();
	});

	it('puts the relationship legend at the upper-right', async () => {
		await openGroupedGraph();
		const legend = screen.getByTestId('graph-legend');
		const panel = legend.closest('.svelte-flow__panel');
		expect(panel?.classList.contains('top')).toBe(true);
		expect(panel?.classList.contains('right')).toBe(true);
	});

	it('stays in the top toolbar in the structured-list alternative', async () => {
		await openGroupedGraph();
		await fireEvent.click(screen.getByLabelText('Structured list'));
		await screen.findByTestId('graph-text');
		const filter = screen.getByTestId('category-filter');
		expect(filter.closest('.toolbar')).not.toBeNull();
		expect(filter.closest('.svelte-flow__panel')).toBeNull();
		expect(screen.queryByTestId('graph-controls')).toBeNull();
	});

	it('renders no category selector in combined mode, tree or graph', async () => {
		await renderLoaded({ trees: { combined: combinedTree } });
		expect(screen.queryByTestId('category-filter')).toBeNull();
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		expect(screen.queryByTestId('category-filter')).toBeNull();
		expect(screen.queryByTestId('category-trigger')).toBeNull();
	});
});

describe('the filtered-empty recovery', () => {
	it('keeps the control available and offers select-all from the popover', async () => {
		await openGroupedGraph();
		await openCategories();
		await clickCategoryAction('Clear all categories');

		const empty = await screen.findByTestId('filtered-empty-state');
		expect(empty.getAttribute('data-state-kind')).toBe('categories-cleared');
		// The graph is gone, so the same compact control is back in the toolbar.
		const filter = screen.getByTestId('category-filter');
		expect(filter.closest('.toolbar')).not.toBeNull();
		expect(screen.queryByTestId('graph')).toBeNull();
		// The pane's own recovery action is still there.
		expect(within(empty).getByTestId('select-all-from-empty')).not.toBeNull();

		await clickCategoryAction('Select all categories');
		await screen.findByTestId('graph');
		await waitFor(() => expect(screen.getAllByTestId(/^graph-node-/).length).toBe(3));
	});
});
