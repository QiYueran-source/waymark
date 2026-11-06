import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import Explorer from '$lib/components/Explorer.svelte';
import TreeView from '$lib/components/TreeView.svelte';
import { allIds } from '$lib/tree';
import { BUG, TASK, bugRoot, combinedTree, groupedTree, makeFetch, taskRoot } from './fixtures';

function renderCombined(overrides: Record<string, unknown> = {}) {
	return render(TreeView, {
		view: 'combined',
		roots: combinedTree.roots,
		groups: [],
		expanded: allIds(combinedTree.roots),
		dependenciesVisible: false,
		selectedId: null,
		onToggle: () => {},
		onSelect: () => {},
		...overrides
	});
}

function renderGrouped(overrides: Record<string, unknown> = {}) {
	return render(TreeView, {
		view: 'grouped',
		roots: [],
		groups: groupedTree.groups,
		expanded: allIds(groupedTree.groups.flatMap((group) => group.roots)),
		dependenciesVisible: false,
		selectedId: null,
		onToggle: () => {},
		onSelect: () => {},
		...overrides
	});
}

describe('dependency overlay in the combined view', () => {
	it('renders no edges unless the overlay is enabled', () => {
		renderCombined();
		expect(screen.queryAllByTestId('dependency-edge')).toHaveLength(0);
	});

	it('renders dashed edges to the referenced items when enabled', () => {
		renderCombined({ dependenciesVisible: true });
		const edges = screen.getAllByTestId('dependency-edge');
		expect(edges).toHaveLength(1);
		expect(edges[0].getAttribute('data-from')).toBe('wm-task-ship');
		expect(edges[0].getAttribute('data-to')).toBe('wm-bug-login');
		expect(edges[0].querySelector('.dashed-line')).not.toBeNull();
		expect(edges[0].textContent).toContain('Login fails');
		expect(edges[0].textContent).toContain('other category');
	});

	it('falls back to the target id when the target is not displayed', () => {
		// A dependency on an id outside the displayed roots still gets a legible
		// edge (an id, not a guessed title).
		const { container } = renderCombined({ dependenciesVisible: true });
		const node = container.querySelector('[data-testid="node-wm-task-ship"]');
		expect(node?.textContent).toContain('wm-bug-login');
	});
});

describe('dependency overlay in the grouped view', () => {
	it('hides cross-category reference nodes unless enabled', () => {
		renderGrouped();
		expect(screen.queryAllByTestId('reference-node')).toHaveLength(0);
	});

	it('adds a reference node for a cross-category target in its parent group', () => {
		renderGrouped({ dependenciesVisible: true });
		const bugs = screen.getByTestId('group-bug');
		const references = within(bugs).getAllByTestId('reference-node');
		expect(references).toHaveLength(1);
		expect(references[0].getAttribute('data-reference-for')).toBe('wm-task-ship');
		expect(references[0].getAttribute('data-reference-to')).toBe('wm-bug-login');
		expect(references[0].textContent).toContain('Ship the release');
		expect(references[0].querySelector('.dashed-line')).not.toBeNull();
	});

	it('names the referenced item when the anchor is its parent', () => {
		// `wm-bug-settings` has a parent, so the reference connects at that parent
		// row and must name the item it stands for.
		const child = {
			...bugRoot.children[0],
			id: 'wm-bug-settings',
			title: 'Settings crash on save',
			category: BUG,
			parent: 'wm-bug-login',
			children: []
		};
		const groups = [
			{ category: BUG, roots: [{ ...bugRoot, children: [child] }] },
			{ category: TASK, roots: [{ ...taskRoot, dependencies: ['wm-bug-settings'] }] }
		];
		renderGrouped({ groups, dependenciesVisible: true });

		const anchor = screen.getByTestId('node-wm-bug-login');
		const references = within(anchor).getAllByTestId('reference-node');
		expect(references).toHaveLength(1);
		expect(references[0].getAttribute('data-reference-to')).toBe('wm-bug-settings');
		expect(references[0].textContent).toContain('Settings crash on save');
		expect(references[0].textContent).toContain('referenced by Ship the release');
	});
});

describe('dependency toggle in the Explorer', () => {
	it('toggles the overlay on and off and fetches nothing extra', async () => {
		const { fetchImpl, urls } = makeFetch({
			trees: { combined: combinedTree, grouped: groupedTree }
		});
		render(Explorer, { fetchImpl, storage: localStorage });
		await screen.findByTestId('node-wm-bug-login');

		const toggle = screen.getByLabelText('Show dependency edges');
		expect((toggle as HTMLInputElement).checked).toBe(false);
		expect(screen.queryAllByTestId('dependency-edge')).toHaveLength(0);

		await fireEvent.click(toggle);
		expect((toggle as HTMLInputElement).checked).toBe(true);
		expect(screen.getAllByTestId('dependency-edge')).toHaveLength(1);

		await fireEvent.click(toggle);
		expect((toggle as HTMLInputElement).checked).toBe(false);
		expect(screen.queryAllByTestId('dependency-edge')).toHaveLength(0);

		// One health read, one combined tree read, and one status read; the
		// overlay is local state.
		expect(urls[0]).toBe('/api/health');
		expect(urls).toContain('/api/tree?view=combined');
		expect(urls).toContain('/api/status');
		expect(urls.filter((url) => url === '/api/tree')).toHaveLength(0);
	});

	it('remembers dependency visibility per view', async () => {
		const { fetchImpl } = makeFetch({
			trees: { combined: combinedTree, grouped: groupedTree }
		});
		render(Explorer, { fetchImpl, storage: localStorage });
		await screen.findByTestId('node-wm-bug-login');

		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		expect(
			(screen.getByLabelText('Show dependency edges') as HTMLInputElement).checked
		).toBe(true);

		await fireEvent.change(screen.getByLabelText('By category'));
		await screen.findByTestId('group-bug');
		// Grouped has never had the overlay enabled, so it starts hidden.
		expect(
			(screen.getByLabelText('Show dependency edges') as HTMLInputElement).checked
		).toBe(false);
		expect(screen.queryAllByTestId('dependency-edge')).toHaveLength(0);

		await fireEvent.change(screen.getByLabelText('Combined'));
		await screen.findByTestId('node-wm-bug-login');
		expect(
			(screen.getByLabelText('Show dependency edges') as HTMLInputElement).checked
		).toBe(true);
	});
});
