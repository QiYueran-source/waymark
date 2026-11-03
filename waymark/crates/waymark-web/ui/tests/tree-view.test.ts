import { render, screen, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import TreeView from '$lib/components/TreeView.svelte';
import { allIds } from '$lib/tree';
import { combinedTree, groupedTree, taskRoot } from './fixtures';

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
