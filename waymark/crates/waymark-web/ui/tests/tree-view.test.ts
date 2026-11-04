import { render, screen, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import TreeView from '$lib/components/TreeView.svelte';
import { allIds, filterGroups } from '$lib/tree';
import { combinedTree, deepGroupedTree, groupedTree, taskRoot } from './fixtures';

function renderCombined(overrides: Record<string, unknown> = {}) {
	return render(TreeView, {
		view: 'combined',
		roots: combinedTree.roots,
		groups: [],
		contextIds: new Set<string>(),
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
		contextIds: new Set<string>(),
		expanded: allIds(groupedTree.groups.flatMap((group) => group.roots)),
		dependenciesVisible: false,
		selectedId: null,
		onToggle: () => {},
		onSelect: () => {},
		...overrides
	});
}

function renderedIds(container: HTMLElement): string[] {
	return Array.from(container.querySelectorAll('[data-testid^="node-"]')).map((element) =>
		(element.getAttribute('data-testid') ?? '').slice('node-'.length)
	);
}

function text(element: Element | null): string {
	return element?.textContent?.replace(/\s+/g, ' ').trim() ?? '';
}

describe('combined tree', () => {
	it('renders parents before children, top-down', () => {
		const { container } = renderCombined();
		// The cross-category child is below its real parent, not promoted to a root.
		expect(renderedIds(container)).toEqual(['wm-bug-login', 'wm-task-child', 'wm-task-ship']);
		expect(screen.getByTestId('node-wm-task-child').getAttribute('data-depth')).toBe('1');
	});

	it('describes each category with its stable shape', () => {
		renderCombined();
		const bugShape = within(screen.getByTestId('node-wm-bug-login')).getByRole('img');
		const taskShape = within(screen.getByTestId('node-wm-task-child')).getByRole('img');
		expect(bugShape.getAttribute('data-shape')).toBe('circle');
		expect(taskShape.getAttribute('data-shape')).toBe('square');
		expect(bugShape.getAttribute('aria-label')).toBe('Bugs category');
		expect(taskShape.getAttribute('aria-label')).toBe('Tasks category');
	});

	it('states the blocked work state in text as well as colour', () => {
		renderCombined();
		expect(text(screen.getByTestId('status-wm-bug-login'))).toBe('In progress');
		expect(text(screen.getByTestId('status-wm-task-ship'))).toBe('Open');
		const state = screen.getByTestId('state-wm-bug-login');
		expect(text(state)).toBe('Blocked');
		expect(state.classList.contains('state-blocked')).toBe(true);
		const reason = screen.getByTestId('blocked-reason-wm-bug-login');
		expect(text(reason)).toBe('! waiting on wm-task-ship (unfinished)');
		// The visual marker is hidden from assistive technology; the reason is prose.
		expect(reason.querySelector('.blocked-marker')?.getAttribute('aria-hidden')).toBe('true');
		expect(text(screen.getByTestId('state-wm-task-ship'))).toBe('Ready');
	});

	it('keeps closed items visible with a text state', () => {
		const closed = { ...taskRoot, status: 'closed' as const, state: 'closed' as const, ready: false };
		renderCombined({ roots: [combinedTree.roots[0], closed] });
		expect(screen.getByTestId('node-wm-task-ship')).not.toBeNull();
		expect(text(screen.getByTestId('state-wm-task-ship'))).toBe('Closed');
	});
});

describe('grouped tree', () => {
	it('groups roots by category and keeps cross-category children under their parent', () => {
		renderGrouped();

		const bugs = screen.getByTestId('group-bug');
		const tasks = screen.getByTestId('group-task');
		expect(within(bugs).queryByTestId('node-wm-bug-login')).not.toBeNull();
		// The child belongs to Tasks but its parent is a Bug, so it renders inside
		// the Bugs group rather than being invented as a Tasks root.
		expect(within(bugs).queryByTestId('node-wm-task-child')).not.toBeNull();
		expect(within(tasks).queryByTestId('node-wm-task-child')).toBeNull();
		expect(within(tasks).queryByTestId('node-wm-task-ship')).not.toBeNull();

		// The child keeps its own category shape, not the group's.
		expect(
			within(bugs)
				.getByTestId('node-wm-task-child')
				.querySelector('[data-shape]')
				?.getAttribute('data-shape')
		).toBe('square');
	});

	it('omits groups with no roots', () => {
		renderGrouped({ groups: groupedTree.groups.filter((group) => group.category.key === 'bug') });
		expect(screen.queryByTestId('group-bug')).not.toBeNull();
		expect(screen.queryByTestId('group-task')).toBeNull();
	});
});

describe('context rows', () => {
	/** Render the grouped view filtered to Tasks, as the Explorer would. */
	function renderTaskScope(groups = groupedTree.groups) {
		const filtered = filterGroups(groups, new Set(['task']));
		return render(TreeView, {
			view: 'grouped',
			roots: [],
			groups: filtered.groups,
			contextIds: filtered.contextIds,
			expanded: allIds(filtered.groups.flatMap((group) => group.roots)),
			dependenciesVisible: false,
			selectedId: null,
			onToggle: () => {},
			onSelect: () => {}
		});
	}

	it('keeps a selected item under its real parent instead of promoting it to a root', () => {
		const { container } = renderTaskScope();
		// The bug is out of scope but stays, because the task's real parent is a bug.
		expect(renderedIds(container)).toEqual([
			'wm-bug-login',
			'wm-task-child',
			'wm-task-ship'
		]);
		expect(screen.getByTestId('node-wm-task-child').getAttribute('data-depth')).toBe('1');
		expect(renderedIds(screen.getByTestId('group-task'))).toEqual(['wm-task-ship']);
	});

	it('keeps the whole ancestor chain when several levels are out of scope', () => {
		const { container } = renderTaskScope(deepGroupedTree.groups);
		expect(renderedIds(container)).toEqual([
			'wm-docs-umbrella',
			'wm-bug-mid',
			'wm-task-leaf',
			'wm-task-ship'
		]);
		expect(screen.getByTestId('node-wm-bug-mid').getAttribute('data-depth')).toBe('1');
		expect(screen.getByTestId('node-wm-task-leaf').getAttribute('data-depth')).toBe('2');
	});

	it('marks a context row in its accessible name and in visible text', () => {
		renderTaskScope();
		const row = screen.getByTestId('node-wm-bug-login');
		expect(row.getAttribute('data-context')).toBe('true');
		// The row's own control says it, so the marking survives a screen reader:
		// focusing the title announces the item and why it is on screen.
		const title = screen.getByTestId('select-wm-bug-login');
		expect(title.getAttribute('aria-label')).toBe(
			'Login fails — context, outside the selection'
		);
		// And the marker is visible text, not colour or an icon alone. It is
		// hidden from assistive technology because the name above already says it.
		const marker = within(row).getByTestId('context-marker-wm-bug-login');
		expect(marker.getAttribute('aria-hidden')).toBe('true');
		expect(text(marker)).toBe('Context — outside the selection');
		// A selected row carries neither.
		const selectedRow = screen.getByTestId('node-wm-task-child');
		expect(selectedRow.getAttribute('data-context')).toBeNull();
		expect(screen.getByTestId('select-wm-task-child').getAttribute('aria-label')).toBeNull();
		expect(within(selectedRow).queryByTestId('context-marker-wm-task-child')).toBeNull();
	});

	it('gives a context ancestor with kept descendants a working toggle', () => {
		renderTaskScope();
		const toggle = screen.getByTestId('toggle-wm-bug-login');
		expect(toggle.getAttribute('aria-expanded')).toBe('true');
		expect(toggle.getAttribute('aria-label')).toBe(
			'Collapse Login fails, context, outside the selection'
		);
	});

	it('keeps a context ancestor collapsed like any other node', () => {
		const filtered = filterGroups(groupedTree.groups, new Set(['task']));
		const { container } = render(TreeView, {
			view: 'grouped',
			roots: [],
			groups: filtered.groups,
			contextIds: filtered.contextIds,
			expanded: allIds([taskRoot]),
			dependenciesVisible: false,
			selectedId: null,
			onToggle: () => {},
			onSelect: () => {}
		});
		// Collapsed: the context ancestor is visible, its kept child is not.
		expect(renderedIds(container)).toEqual(['wm-bug-login', 'wm-task-ship']);
		expect(screen.getByTestId('toggle-wm-bug-login').getAttribute('aria-expanded')).toBe('false');
	});

	it('leaves the combined view unmarked, since it filters nothing', () => {
		const { container } = renderCombined({ contextIds: new Set(['wm-bug-login']) });
		expect(container.querySelectorAll('[data-context]')).toHaveLength(0);
		expect(screen.getByTestId('select-wm-bug-login').getAttribute('aria-label')).toBeNull();
	});
});

describe('expansion', () => {
	it('hides children not in the expanded set', () => {
		const { container } = renderCombined({ expanded: [] });
		expect(renderedIds(container)).toEqual(['wm-bug-login', 'wm-task-ship']);
		expect(screen.getByTestId('toggle-wm-bug-login').getAttribute('aria-expanded')).toBe('false');
	});

	it('marks expanded parents for assistive technology', () => {
		renderCombined();
		const toggle = screen.getByTestId('toggle-wm-bug-login');
		expect(toggle.getAttribute('aria-expanded')).toBe('true');
		expect(toggle.getAttribute('aria-label')).toBe('Collapse Login fails');
	});

	it('does not render a toggle for leaves', () => {
		renderCombined();
		expect(screen.queryByTestId('toggle-wm-task-ship')).toBeNull();
	});

	it('marks the selected row with aria-current', () => {
		renderCombined({ selectedId: 'wm-task-ship' });
		expect(screen.getByTestId('select-wm-task-ship').getAttribute('aria-current')).toBe('true');
		expect(screen.getByTestId('select-wm-bug-login').getAttribute('aria-current')).toBeNull();
	});
});

describe('empty tree', () => {
	it('renders no rows for an empty root list', () => {
		const { container } = renderCombined({ roots: [] });
		expect(container.querySelectorAll('[data-testid^="node-"]')).toHaveLength(0);
	});
});
