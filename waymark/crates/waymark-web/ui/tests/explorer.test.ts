import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import Explorer from '$lib/components/Explorer.svelte';
import { projectIdFor } from '$lib/storage';
import {
	ROOT_A,
	ROOT_B,
	combinedTree,
	detail,
	emptyTree,
	failure,
	groupedTree,
	makeFetch,
	unresolvedTree
} from './fixtures';

async function renderLoaded(options: Parameters<typeof makeFetch>[0]) {
	const api = makeFetch(options);
	const view = render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
	await screen.findByTestId('node-wm-bug-login');
	return { ...api, ...view };
}

function checked(label: string): boolean {
	return (screen.getByLabelText(label) as HTMLInputElement).checked;
}

describe('loading and project identity', () => {
	it('shows the normalized project root from /api/health', async () => {
		const { urls } = await renderLoaded({ trees: { combined: combinedTree } });
		expect(screen.getByTestId('project-root').textContent).toBe(ROOT_A);
		expect(checked('Combined')).toBe(true);
		expect(urls[0]).toBe('/api/health');
	});

	it('reads the combined view by default', async () => {
		const { urls } = await renderLoaded({ trees: { combined: combinedTree } });
		expect(urls).toEqual(['/api/health', '/api/tree?view=combined']);
		expect(screen.getByTestId('node-wm-task-child')).not.toBeNull();
	});

	it('states how much work there is and what is held up', async () => {
		await renderLoaded({ trees: { combined: combinedTree } });
		// Three items in the fixture, one of them blocked.
		expect(screen.getByTestId('tree-summary').textContent).toBe('3 items · 1 blocked');
	});
});

describe('empty and unresolved states', () => {
	it('shows a legible empty state, not a blank tree', async () => {
		const api = makeFetch({ trees: { combined: emptyTree } });
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
		const empty = await screen.findByTestId('empty-state');
		expect(empty.textContent).toContain('No active work items');
		expect(screen.queryAllByTestId('dependency-edge')).toHaveLength(0);
		expect(screen.getByTestId('tree-summary').textContent).toBe('0 items · 0 blocked');
	});

	it('lists known items whose parent relation is unresolved', async () => {
		await renderLoaded({ trees: { combined: unresolvedTree } });
		const unresolved = screen.getByTestId('unresolved');
		expect(unresolved.textContent).toContain('Orphaned work');
		expect(unresolved.textContent).toContain('its parent wm-missing is missing');
		// The unresolved item is not invented as a root in the tree.
		expect(screen.queryByTestId('node-wm-orphan')).toBeNull();
	});
});

describe('read failure', () => {
	it('shows the INCOMPLETE_PROJECTION envelope and diagnostics instead of a partial tree', async () => {
		const api = makeFetch({
			treeFailure: failure('INCOMPLETE_PROJECTION', 'the project projection is incomplete', [
				'work/tasks/wm-broken.md: damaged document',
				{ path: 'work/tasks/wm-broken.md', reason: 'parse' }
			])
		});
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
		const readFailure = await screen.findByTestId('read-failure');
		expect(screen.getByTestId('read-failure-code').textContent).toContain(
			'INCOMPLETE_PROJECTION'
		);
		expect(screen.getByTestId('read-failure-message').textContent).toContain(
			'the project projection is incomplete'
		);
		const diagnostics = screen.getByTestId('read-failure-diagnostics');
		expect(diagnostics.textContent).toContain('work/tasks/wm-broken.md: damaged document');
		expect(diagnostics.textContent).toContain('"reason":"parse"');
		expect(screen.getByTestId('read-failure').textContent).toContain('No partial tree is shown');
		// Nothing from a previous or imagined read leaks into the failure state.
		expect(screen.queryByTestId('node-wm-bug-login')).toBeNull();
		expect(screen.queryByTestId('empty-state')).toBeNull();
		expect(readFailure.getAttribute('role')).toBe('alert');
	});

	it('shows a health read failure when the service cannot be read at all', async () => {
		const api = makeFetch({ healthFailure: failure('PROJECT_NOT_FOUND', 'no project here') });
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
		await screen.findByTestId('read-failure');
		expect(screen.getByTestId('read-failure-code').textContent).toContain('PROJECT_NOT_FOUND');
	});
});

describe('expansion', () => {
	it('collapses and expands a node, and expand/collapse all', async () => {
		await renderLoaded({ trees: { combined: combinedTree } });
		expect(screen.getByTestId('node-wm-task-child')).not.toBeNull();

		await fireEvent.click(screen.getByTestId('toggle-wm-bug-login'));
		expect(screen.queryByTestId('node-wm-task-child')).toBeNull();
		expect(screen.getByTestId('toggle-wm-bug-login').getAttribute('aria-expanded')).toBe('false');

		await fireEvent.click(screen.getByTestId('toggle-wm-bug-login'));
		expect(screen.getByTestId('node-wm-task-child')).not.toBeNull();

		await fireEvent.click(screen.getByRole('button', { name: 'Collapse all' }));
		expect(screen.queryByTestId('node-wm-task-child')).toBeNull();
		await fireEvent.click(screen.getByRole('button', { name: 'Expand all' }));
		expect(screen.getByTestId('node-wm-task-child')).not.toBeNull();
	});

	it('is per view: expansion in combined does not change grouped', async () => {
		await renderLoaded({ trees: { combined: combinedTree, grouped: groupedTree } });
		await fireEvent.click(screen.getByTestId('toggle-wm-bug-login'));
		expect(screen.queryByTestId('node-wm-task-child')).toBeNull();

		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('group-bug');
		// Grouped starts fully expanded because it has its own stored set.
		expect(screen.getByTestId('node-wm-task-child')).not.toBeNull();
	});
});

describe('browser-state persistence', () => {
	it('restores view, expansion, dependency visibility, and scroll after a reload', async () => {
		const first = await renderLoaded({
			trees: { combined: combinedTree, grouped: groupedTree }
		});
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		await fireEvent.click(screen.getByTestId('toggle-wm-bug-login'));
		const pane = screen.getByLabelText('Work items');
		pane.scrollTop = 140;
		await fireEvent.scroll(pane);
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('group-bug');
		first.unmount();

		const second = makeFetch({ trees: { combined: combinedTree, grouped: groupedTree } });
		render(Explorer, { fetchImpl: second.fetchImpl, storage: localStorage });
		await screen.findByTestId('group-bug');
		expect(checked('By category')).toBe(true);

		await fireEvent.click(screen.getByLabelText('Combined'));
		await screen.findByTestId('node-wm-bug-login');
		expect(checked('Show dependency edges')).toBe(true);
		expect(screen.queryByTestId('node-wm-task-child')).toBeNull();
		await waitFor(() => {
			expect(screen.getByLabelText('Work items').scrollTop).toBe(140);
		});
	});

	it('isolates preferences per project on one origin', async () => {
		const first = await renderLoaded({ root: ROOT_A, trees: { combined: combinedTree } });
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		expect(checked('Show dependency edges')).toBe(true);
		first.unmount();

		const second = makeFetch({ root: ROOT_B, trees: { combined: combinedTree } });
		render(Explorer, { fetchImpl: second.fetchImpl, storage: localStorage });
		await screen.findByTestId('node-wm-bug-login');
		// A fresh project does not inherit the other project's overlay choice.
		expect(checked('Show dependency edges')).toBe(false);

		// Changing it here writes only B's namespace; A's stored value is untouched.
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		expect(checked('Show dependency edges')).toBe(true);

		const keys = Object.keys(localStorage);
		expect(keys.some((key) => key.startsWith(projectIdFor(ROOT_A)))).toBe(true);
		expect(keys.some((key) => key.startsWith(projectIdFor(ROOT_B)))).toBe(true);
		expect(projectIdFor(ROOT_A)).not.toBe(projectIdFor(ROOT_B));
		const aOverlay = keys.find(
			(key) => key.startsWith(projectIdFor(ROOT_A)) && key.endsWith(':dependencies:combined')
		);
		expect(aOverlay).toBeDefined();
		expect(localStorage.getItem(aOverlay as string)).toBe('true');
	});

	it('keeps working in memory with a notice when storage is unavailable', async () => {
		const api = makeFetch({ trees: { combined: combinedTree } });
		render(Explorer, { fetchImpl: api.fetchImpl, storage: null });
		await screen.findByTestId('node-wm-bug-login');

		expect(screen.getByTestId('persistence-notice').textContent).toContain(
			'Preferences could not be saved'
		);
		// The view is still usable for the session.
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		expect(checked('Show dependency edges')).toBe(true);
		await fireEvent.click(screen.getByTestId('toggle-wm-bug-login'));
		expect(screen.queryByTestId('node-wm-task-child')).toBeNull();
	});

	it('warns without breaking when saving fails', async () => {
		const failing = {
			length: 0,
			key: () => null,
			getItem: () => null,
			setItem: () => {
				throw new Error('quota exceeded');
			},
			removeItem: () => {},
			clear: () => {}
		} as unknown as Storage;
		const api = makeFetch({ trees: { combined: combinedTree } });
		render(Explorer, { fetchImpl: api.fetchImpl, storage: failing });
		await screen.findByTestId('node-wm-bug-login');
		expect(screen.queryByTestId('persistence-notice')).toBeNull();

		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		expect(checked('Show dependency edges')).toBe(true);
		expect(screen.getByTestId('persistence-notice')).not.toBeNull();
	});

	it('raises the persistence notice once, however many writes fail', async () => {
		let failedWrites = 0;
		const failing = {
			length: 0,
			key: () => null,
			getItem: () => null,
			setItem: () => {
				failedWrites += 1;
				throw new Error('quota exceeded');
			},
			removeItem: () => {},
			clear: () => {}
		} as unknown as Storage;
		const api = makeFetch({ trees: { combined: combinedTree } });
		render(Explorer, { fetchImpl: api.fetchImpl, storage: failing });
		await screen.findByTestId('node-wm-bug-login');

		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		await fireEvent.click(screen.getByTestId('toggle-wm-bug-login'));
		await fireEvent.click(screen.getByRole('button', { name: 'Collapse all' }));

		expect(failedWrites).toBeGreaterThan(1);
		expect(screen.getAllByTestId('persistence-notice')).toHaveLength(1);
		expect(screen.getByTestId('persistence-notice').getAttribute('role')).toBe('status');
	});
});

describe('detail selection', () => {
	it('loads the selected item without disturbing the tree position or expansion', async () => {
		const { urls } = await renderLoaded({
			trees: { combined: combinedTree },
			items: { 'wm-bug-login': detail }
		});
		await fireEvent.click(screen.getByTestId('select-wm-bug-login'));
		await screen.findByTestId('detail-panel');
		await waitFor(() => {
			expect(screen.getByTestId('detail-title').textContent).toBe('<b>Login</b> fails');
		});
		expect(urls).toContain('/api/item/wm-bug-login');
		// Tree expansion and rows are untouched by opening a detail.
		expect(screen.getByTestId('node-wm-task-child')).not.toBeNull();

		await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
		expect(screen.queryByTestId('detail-panel')).toBeNull();
	});
});

describe('keyboard focus in the detail panel', () => {
	it('moves focus into the panel, then back to the row that opened it', async () => {
		await renderLoaded({
			trees: { combined: combinedTree },
			items: { 'wm-bug-login': detail }
		});
		const trigger = screen.getByTestId('select-wm-bug-login');
		trigger.focus();
		await fireEvent.click(trigger);

		const panel = await screen.findByTestId('detail-panel');
		expect(panel.getAttribute('tabindex')).toBe('-1');
		await waitFor(() => expect(document.activeElement).toBe(panel));

		// The panel is labelled by the item title, so it reads as the detail.
		expect(panel.getAttribute('aria-labelledby')).toBe('detail-title');
		await waitFor(() => {
			expect(screen.getByTestId('detail-title').textContent).toBe('<b>Login</b> fails');
		});

		await fireEvent.keyDown(panel, { key: 'Escape' });
		await waitFor(() => expect(screen.queryByTestId('detail-panel')).toBeNull());
		await waitFor(() => expect(document.activeElement).toBe(trigger));
	});

	it('leaves Escape to the tree while focus is outside the panel', async () => {
		await renderLoaded({
			trees: { combined: combinedTree },
			items: { 'wm-bug-login': detail }
		});
		await fireEvent.click(screen.getByTestId('select-wm-bug-login'));
		const panel = await screen.findByTestId('detail-panel');
		// Wait for the read to settle, so its content update cannot move focus.
		await waitFor(() => expect(document.activeElement).toBe(panel));

		const treeButton = screen.getByTestId('select-wm-task-ship');
		treeButton.focus();
		await fireEvent.keyDown(treeButton, { key: 'Escape' });

		// Escape in the tree does not close the detail, and the row keeps focus.
		expect(screen.getByTestId('detail-panel')).not.toBeNull();
		expect(document.activeElement).toBe(treeButton);
	});
});
