import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import Explorer from '$lib/components/Explorer.svelte';
import { preferenceKey, projectIdFor } from '$lib/storage';
import {
	ROOT_A,
	ROOT_B,
	combinedTree,
	deepGroupedTree,
	dependencyTree,
	detail,
	groupedTree,
	hierarchyTree,
	makeFetch,
	unresolvedTree
} from './fixtures';

type FetchOptions = Parameters<typeof makeFetch>[0];

async function renderLoaded(options: FetchOptions) {
	const api = makeFetch(options);
	const view = render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
	await screen.findByTestId('tree-summary');
	return { ...api, ...view };
}

/** Switch to the graph, then to its structured text alternative. */
async function openAlternative(options: FetchOptions) {
	const rendered = await renderLoaded(options);
	await fireEvent.click(screen.getByLabelText('Graph'));
	await fireEvent.click(screen.getByLabelText('Structured list'));
	await screen.findByTestId('graph-text');
	return rendered;
}

function alternative(): HTMLElement {
	return screen.getByTestId('graph-text');
}

function textOf(element: Element | null): string {
	return element?.textContent?.replace(/\s+/g, ' ').trim() ?? '';
}

/** The alternative's items, in reading order. */
function entries(): HTMLElement[] {
	return screen.getAllByTestId(/^graph-text-item-/);
}

function entry(id: string): HTMLElement {
	return screen.getByTestId(`graph-text-item-${id}`);
}

describe('the structured text alternative control', () => {
	it('toggles between the drawing and its text alternative, and back', async () => {
		await renderLoaded({ trees: { combined: combinedTree } });
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		// The alternative is offered beside the graph, but not while the tree
		// is the presentation.
		expect(screen.getByLabelText('Structured list')).not.toBeNull();

		await fireEvent.click(screen.getByLabelText('Structured list'));
		await screen.findByTestId('graph-text');
		// One or the other, never both.
		expect(screen.queryByTestId('graph')).toBeNull();
		expect(entries().map((element) => element.dataset.nodeId)).toEqual([
			'wm-bug-login',
			'wm-task-child',
			'wm-task-ship'
		]);

		await fireEvent.click(screen.getByLabelText('Structured list'));
		await screen.findByTestId('graph');
		expect(screen.queryByTestId('graph-text')).toBeNull();

		await fireEvent.click(screen.getByLabelText('Tree'));
		await screen.findByTestId('node-wm-bug-login');
		expect(screen.queryByLabelText('Structured list')).toBeNull();
	});

	it('remembers the choice per project, and restores it with the graph', async () => {
		const first = await openAlternative({ trees: { combined: combinedTree } });
		expect(localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'graphAlternative'))).toBe(
			'true'
		);
		first.unmount();

		// The same project reopens on the graph's alternative, with no extra read.
		const second = await renderLoaded({ trees: { combined: combinedTree } });
		await screen.findByTestId('graph-text');
		expect(screen.queryByTestId('graph')).toBeNull();
		expect((screen.getByLabelText('Structured list') as HTMLInputElement).checked).toBe(true);
		expect(second.urls).toEqual(['/api/health', '/api/tree?view=combined']);
		second.unmount();

		// Another project sharing the origin starts on the drawing.
		const third = makeFetch({ root: ROOT_B, trees: { combined: combinedTree } });
		render(Explorer, { fetchImpl: third.fetchImpl, storage: localStorage });
		await screen.findByTestId('tree-summary');
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		expect(screen.queryByTestId('graph-text')).toBeNull();
	});
});

describe('what the alternative says', () => {
	it('states every drawn item with its category, state, and status in words', async () => {
		await openAlternative({ trees: { combined: hierarchyTree } });
		expect(entries().map((element) => element.dataset.nodeId)).toEqual([
			'wm-h-root',
			'wm-h-child',
			'wm-h-grandchild',
			'wm-h-second-root'
		]);
		const root = textOf(entry('wm-h-root'));
		expect(root).toContain('Crashes on start');
		expect(root).toContain('Category: Bugs');
		expect(root).toContain('State: Blocked');
		expect(root).toContain('Status: In progress');
		// The server's blocked reason is prose, not a marker.
		expect(root).toContain('waiting on wm-h-child (unfinished)');
		expect(textOf(entry('wm-h-second-root'))).toContain('State: Ready');
		expect(textOf(entry('wm-h-second-root'))).toContain('Category: Tasks');
	});

	it('states parent-child relations in words instead of leaving them to indentation', async () => {
		await openAlternative({ trees: { combined: hierarchyTree } });
		expect(textOf(entry('wm-h-root'))).toContain('Parent: none; this is a top-level item.');
		expect(textOf(entry('wm-h-root'))).toContain('Child: Reproduce the crash (wm-h-child).');
		expect(textOf(entry('wm-h-child'))).toContain('Parent: Crashes on start (wm-h-root).');
		expect(textOf(entry('wm-h-child'))).toContain('Child: Verify the fix (wm-h-grandchild).');
		expect(textOf(entry('wm-h-grandchild'))).toContain(
			'Parent: Reproduce the crash (wm-h-child).'
		);
		expect(textOf(entry('wm-h-grandchild'))).not.toContain('Child:');
	});

	it('shows dependency relations only while dependency visibility is on, with their direction', async () => {
		const rendered = await renderLoaded({ trees: { combined: dependencyTree } });
		await fireEvent.click(screen.getByLabelText('Graph'));
		await fireEvent.click(screen.getByLabelText('Structured list'));
		await screen.findByTestId('graph-text');

		// Off by default: no dependency sentence anywhere.
		expect(alternative().textContent).not.toMatch(/depend/i);

		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		await waitFor(() => expect(alternative().textContent).toMatch(/Depends on/));
		expect(textOf(entry('wm-dep-fix'))).toContain('Depends on Login fails (wm-dep-bug).');
		expect(textOf(entry('wm-dep-bug'))).toContain(
			'Fix the login form (wm-dep-fix) depends on this item.'
		);
		expect(textOf(entry('wm-dep-bug'))).toContain(
			'Ship the release (wm-dep-ship) depends on this item.'
		);
		// The relation is stated once per item, in both directions.
		expect(textOf(entry('wm-dep-ship'))).toContain('Depends on Login fails (wm-dep-bug).');
		// A dependency whose target is not drawn gets no sentence.
		expect(alternative().textContent).not.toContain('wm-dep-archived');
		expect(rendered.requests.every((request) => request.method === 'GET')).toBe(true);
	});

	it('lists the unresolved items with the reason their parent cannot be reached', async () => {
		await openAlternative({ trees: { combined: unresolvedTree } });
		const orphan = screen.getByTestId('graph-text-unresolved-wm-orphan');
		expect(textOf(orphan)).toContain('Orphaned work');
		expect(textOf(orphan)).toContain('Category: Tasks');
		expect(textOf(orphan)).toContain('its parent wm-missing is missing');
		// It is not drawn as an ordinary root, and the separate unresolved area
		// is not needed while the alternative already accounts for it.
		expect(entries().map((element) => element.dataset.nodeId)).not.toContain('wm-orphan');
		expect(screen.queryByTestId('unresolved')).toBeNull();
	});
});

describe('parity with the graph', () => {
	it('applies the same category scope as the graph and the grouped tree', async () => {
		await renderLoaded({ trees: { combined: combinedTree, grouped: deepGroupedTree } });
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('category-filter');
		await fireEvent.click(screen.getByLabelText('Graph'));
		await fireEvent.click(screen.getByLabelText('Structured list'));
		await screen.findByTestId('graph-text');
		expect(entries().map((element) => element.dataset.nodeId)).toEqual([
			'wm-docs-umbrella',
			'wm-bug-mid',
			'wm-task-leaf',
			'wm-bug-sibling',
			'wm-docs-sibling',
			'wm-task-ship'
		]);

		// Clear Tasks: those items leave both presentations, and the ancestors
		// they needed leave with them.
		await fireEvent.click(screen.getByRole('button', { name: 'Clear all categories' }));
		await fireEvent.click(screen.getByTestId('category-option-bug'));
		await waitFor(() =>
			expect(entries().map((element) => element.dataset.nodeId)).toEqual([
				'wm-docs-umbrella',
				'wm-bug-mid',
				'wm-bug-sibling'
			])
		);
		expect(alternative().textContent).not.toContain('Polish the leaf');

		// The drawing is built from the same model, so it shows the same set.
		await fireEvent.click(screen.getByLabelText('Structured list'));
		await screen.findByTestId('graph');
		expect(
			screen.getAllByTestId(/^graph-node-/).map((element) => element.dataset.nodeId)
		).toEqual(['wm-docs-umbrella', 'wm-bug-mid', 'wm-bug-sibling']);
	});

	it('is built from the graph’s model, so it can never show an item the drawing hides', async () => {
		await openAlternative({ trees: { combined: hierarchyTree } });
		const fromAlternative = entries().map((element) => element.dataset.nodeId);
		await fireEvent.click(screen.getByLabelText('Structured list'));
		await screen.findByTestId('graph');
		const fromGraph = screen
			.getAllByTestId(/^graph-node-/)
			.map((element) => element.dataset.nodeId);
		expect(fromAlternative).toEqual(fromGraph);
	});
});

describe('keyboard and plain-text reading', () => {
	it('opens the same read-only detail from an alternative entry by keyboard', async () => {
		await openAlternative({
			trees: { combined: combinedTree },
			items: { 'wm-bug-login': detail }
		});
		const trigger = screen.getByTestId('graph-text-select-wm-bug-login');
		expect(trigger.tagName).toBe('BUTTON');
		expect(trigger.tabIndex).toBe(0);
		// The accessible name keeps the category and state the card would show.
		expect(trigger.getAttribute('aria-label')).toBe('Login fails — Bugs, Blocked');

		trigger.focus();
		expect(document.activeElement).toBe(trigger);
		await fireEvent.keyDown(trigger, { key: 'Enter' });

		const panel = await screen.findByTestId('detail-panel');
		await waitFor(() =>
			expect(screen.getByTestId('detail-title').textContent).toBe('<b>Login</b> fails')
		);
		expect(trigger.getAttribute('aria-current')).toBe('true');
		await waitFor(() => expect(document.activeElement).toBe(panel));

		// Escape closes it and focus returns to the same entry.
		await fireEvent.keyDown(panel, { key: 'Escape' });
		await waitFor(() => expect(screen.queryByTestId('detail-panel')).toBeNull());
		await waitFor(() => expect(document.activeElement).toBe(trigger));
		expect(trigger.getAttribute('aria-current')).toBeNull();
	});

	it('reads as a coherent outline without colour, shape, or geometry', async () => {
		await openAlternative({ trees: { combined: hierarchyTree } });
		const full = textOf(alternative());
		// Every relation and state is present as a sentence.
		for (const phrase of [
			'Parent: Crashes on start (wm-h-root).',
			'Child: Verify the fix (wm-h-grandchild).',
			'State: Blocked',
			'State: Ready',
			'Category: Bugs',
			'Category: Tasks'
		]) {
			expect(full).toContain(phrase);
		}
		// No shape glyph or colour-carrying class is the only cue: nothing in
		// the alternative renders a shape or an SVG.
		expect(alternative().querySelector('svg, .shape')).toBeNull();
		// Items are a real list, so the reading order is programmatic too.
		expect(alternative().querySelectorAll('ol > li')).toHaveLength(4);
	});
});
