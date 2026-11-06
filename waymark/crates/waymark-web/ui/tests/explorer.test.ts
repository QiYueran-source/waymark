import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import Explorer from '$lib/components/Explorer.svelte';
import { preferenceKey, projectIdFor } from '$lib/storage';
import {
	ROOT_A,
	ROOT_B,
	TASK,
	combinedTree,
	deepGroupedTree,
	detail,
	emptyTree,
	failure,
	finding,
	groupedTree,
	groupedTreeWithEmptyCategory,
	healthyStatus,
	incompleteProjectionSummary,
	inventoryUncertaintyFinding,
	makeFetch,
	neverLoadedStatus,
	parseErrorFinding,
	staleStatus,
	unresolvedTree,
	validationFinding
} from './fixtures';
import {
	categoryOption,
	clickCategory,
	clickCategoryAction,
	openCategories
} from './category-helpers';


/**
 * The one sentence the projection's summary finding and its error both use.
 * Counts are part of it, so a fixture that changes them changes this string.
 */
const INCOMPLETE_SENTENCE =
	'a complete, fresh projection is required but the projection is incomplete: 1 document(s) could not be read and 0 item-shaped path(s) are uncertain';

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
		expect(urls[0]).toBe('/api/health');
		expect(urls).toContain('/api/tree?view=combined');
		expect(urls).toContain('/api/status');
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

	it('names the offending document and field when a validation finding blocks the projection', async () => {
		const api = makeFetch({
			treeFailure: failure('INCOMPLETE_PROJECTION', INCOMPLETE_SENTENCE, [
				incompleteProjectionSummary({ documents: 1 }),
				validationFinding('work/tasks/wm-broken.md', 'status', 'unknown status "started"')
			])
		});
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
		await screen.findByTestId('read-failure');

		const documents = screen.getByTestId('read-failure-documents');
		expect(documents.textContent).toContain('work/tasks/wm-broken.md');
		// The document path comes from the location itself, not only from the
		// message, and the field keeps its own label.
		expect(documents.querySelector('.problem-where')?.textContent).toBe(
			'work/tasks/wm-broken.md'
		);
		expect(documents.textContent).toContain('field');
		expect(documents.querySelector('.problem-field')?.textContent).toContain('status');
		expect(documents.textContent).toContain('unknown status "started"');

		// The projection's own count-bearing summary is not a document problem, so
		// it is not listed as one.
		expect(documents.textContent).not.toContain('item-shaped path(s)');

		const guidance = screen.getByTestId('read-failure-guidance').textContent ?? '';
		expect(guidance).toContain('read-only');
		expect(guidance).toContain('waymark validate');

		// Findings are rendered as prose, never as a wall of JSON.
		expect(screen.queryByTestId('read-failure-diagnostics')).toBeNull();
		expect(screen.queryByTestId('node-wm-bug-login')).toBeNull();
		expect(screen.queryByTestId('empty-state')).toBeNull();
	});

	it('names the document and the first failing field when several fields are wrong', async () => {
		// The message the projection actually sends for a multi-field validation
		// failure: the document and the issue count in prose, the first offending
		// field in the location. The path is still on screen, and the guidance
		// points at the command that lists every issue.
		const api = makeFetch({
			treeFailure: failure('INCOMPLETE_PROJECTION', INCOMPLETE_SENTENCE, [
				incompleteProjectionSummary({ documents: 1 }),
				finding({
					code: 'VALIDATION_ERROR',
					location: { kind: 'field', field: 'created_at' },
					message: 'work item document work/tasks/wm-broken.md: 2 validation issues'
				})
			])
		});
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
		await screen.findByTestId('read-failure');

		const documents = screen.getByTestId('read-failure-documents');
		expect(documents.textContent).toContain('work/tasks/wm-broken.md');
		expect(documents.textContent).toContain('created_at');
		expect(documents.textContent).toContain('2 validation issues');
		// A field location without a path is still accepted (the wire shape is
		// additive); the document stays named by the message.
		expect(documents.querySelector('.problem-where')).toBeNull();
		expect(screen.getByTestId('read-failure-guidance').textContent).toContain('waymark validate');
	});

	it('separates documents that could not be read from uncertain inventory paths', async () => {
		const both = makeFetch({
			treeFailure: failure('INCOMPLETE_PROJECTION', 'incomplete', [
				incompleteProjectionSummary({ documents: 2, uncertain: 1 }),
				parseErrorFinding('work/tasks/wm-broken.md', 'missing field `status`'),
				finding({
					code: 'IO_ERROR',
					location: { kind: 'file', path: 'work/bugs/wm-unreadable.md' },
					message: 'cannot read work/bugs/wm-unreadable.md: permission denied'
				}),
				inventoryUncertaintyFinding('work/bugs/wm-orphan.md', 'not a stable ID')
			])
		});
		const first = render(Explorer, { fetchImpl: both.fetchImpl, storage: localStorage });
		await screen.findByTestId('read-failure');

		const documents = screen.getByTestId('read-failure-documents');
		expect(documents.textContent).toContain('work/tasks/wm-broken.md');
		expect(documents.textContent).toContain('PARSE_ERROR');
		expect(documents.textContent).toContain('work/bugs/wm-unreadable.md');
		expect(documents.textContent).toContain('IO_ERROR');

		const uncertain = screen.getByTestId('read-failure-uncertain');
		expect(uncertain.textContent).toContain('work/bugs/wm-orphan.md');
		expect(uncertain.textContent).toContain('not a stable ID');
		// Each cause keeps its own heading and its own findings.
		expect(uncertain.textContent).not.toContain('work/tasks/wm-broken.md');
		expect(documents.textContent).not.toContain('work/bugs/wm-orphan.md');
		first.unmount();

		const unreadableOnly = makeFetch({
			treeFailure: failure('INCOMPLETE_PROJECTION', 'incomplete', [
				incompleteProjectionSummary({ documents: 1, uncertain: 0 }),
				parseErrorFinding('work/tasks/wm-broken.md', 'missing field `status`')
			])
		});
		render(Explorer, { fetchImpl: unreadableOnly.fetchImpl, storage: localStorage });
		await screen.findByTestId('read-failure');
		expect(screen.queryByTestId('read-failure-uncertain')).toBeNull();
	});

	it('reports uncertain inventory paths even when no document failed to load', async () => {
		const api = makeFetch({
			treeFailure: failure('INCOMPLETE_PROJECTION', 'incomplete', [
				incompleteProjectionSummary({ documents: 0, uncertain: 1 }),
				inventoryUncertaintyFinding('work/bugs/wm-Orphan.md', 'the stem is not lowercase')
			])
		});
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
		await screen.findByTestId('read-failure');

		const uncertain = screen.getByTestId('read-failure-uncertain');
		expect(uncertain.textContent).toContain('work/bugs/wm-Orphan.md');
		expect(uncertain.textContent).toContain('the inventory is uncertain');
		expect(screen.queryByTestId('read-failure-documents')).toBeNull();
	});

	it('renders plain-string and foreign diagnostics without inventing findings', async () => {
		const api = makeFetch({
			treeFailure: failure('INCOMPLETE_PROJECTION', 'incomplete', [
				'plain diagnostic',
				{ code: 42, message: null },
				{ path: 'work/tasks/wm-broken.md', reason: 'parse' }
			])
		});
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
		await screen.findByTestId('read-failure');

		const diagnostics = screen.getByTestId('read-failure-diagnostics');
		expect(diagnostics.textContent).toContain('plain diagnostic');
		expect(diagnostics.textContent).toContain('"reason":"parse"');
		expect(screen.queryByTestId('read-failure-documents')).toBeNull();
		expect(screen.queryByTestId('read-failure-uncertain')).toBeNull();
		// Guidance is still true: the projection code says the read was blocked.
		expect(screen.getByTestId('read-failure-guidance').textContent).toContain('waymark validate');
	});

	it('keeps the inventory findings that reuse a document code in the document group', async () => {
		// A case variant of a live ID is reported as DUPLICATE_ID, and a
		// non-lowercase stem with no counterpart as PARSE_ERROR: the projection
		// reuses those codes on purpose, so they stay document findings here and
		// their paths are still named.
		const api = makeFetch({
			treeFailure: failure('INCOMPLETE_PROJECTION', 'incomplete', [
				incompleteProjectionSummary({ documents: 0, uncertain: 2 }),
				finding({
					code: 'DUPLICATE_ID',
					location: { kind: 'file', path: 'work/tasks/wm-AAAAAAAAAA12.md' },
					message:
						'the case variant work/tasks/wm-AAAAAAAAAA12.md claims the same ID as work/tasks/wm-aaaaaaaaaa12.md'
				}),
				finding({
					code: 'PARSE_ERROR',
					location: { kind: 'file', path: 'work/tasks/wm-AAAAAAAAAA99.md' },
					message:
						'the document path work/tasks/wm-AAAAAAAAAA99.md is not the lowercase form of its stable ID'
				})
			])
		});
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
		await screen.findByTestId('read-failure');

		const documents = screen.getByTestId('read-failure-documents');
		expect(documents.textContent).toContain('work/tasks/wm-AAAAAAAAAA12.md');
		expect(documents.textContent).toContain('DUPLICATE_ID');
		expect(documents.textContent).toContain('work/tasks/wm-AAAAAAAAAA99.md');
		expect(documents.textContent).toContain('PARSE_ERROR');
		// Nothing is dropped into the fallback list just because it is unusual.
		expect(screen.queryByTestId('read-failure-diagnostics')).toBeNull();
	});

	it('files located findings that name no document under their own heading', async () => {
		const api = makeFetch({
			treeFailure: failure('INCOMPLETE_PROJECTION', 'incomplete', [
				incompleteProjectionSummary({ documents: 0, uncertain: 0 }),
				finding({
					code: 'CONFIG_INVALID',
					location: { kind: 'config_key', key: 'categories' },
					message: 'the key "categories" is invalid'
				})
			])
		});
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
		await screen.findByTestId('read-failure');

		const other = screen.getByTestId('read-failure-other');
		expect(other.textContent).toContain('CONFIG_INVALID');
		expect(other.textContent).toContain('categories');
		expect(screen.queryByTestId('read-failure-documents')).toBeNull();
		expect(screen.queryByTestId('read-failure-uncertain')).toBeNull();
	});

	it('shows a health read failure when the service cannot be read at all', async () => {
		const api = makeFetch({ healthFailure: failure('PROJECT_NOT_FOUND', 'no project here') });
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
		await screen.findByTestId('read-failure');
		expect(screen.getByTestId('read-failure-code').textContent).toContain('PROJECT_NOT_FOUND');
		// A transport failure is not a blocked projection, so the block offers no
		// project-repair advice it cannot stand behind.
		expect(screen.queryByTestId('read-failure-guidance')).toBeNull();
		expect(screen.getByTestId('read-failure').textContent).not.toContain('waymark validate');
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

/** Render, then switch to the by-category view and wait for its first group. */
async function renderGrouped(options: Parameters<typeof makeFetch>[0]) {
	const rendered = await renderLoaded(options);
	await fireEvent.click(screen.getByLabelText('By category'));
	await screen.findByTestId('group-bug');
	return rendered;
}

async function optionLabel(key: string): Promise<string> {
	return (await categoryOption(key)).getAttribute('aria-label') ?? '';
}

async function pressed(key: string): Promise<string | null> {
	return (await categoryOption(key)).getAttribute('aria-pressed');
}

describe('category selection', () => {
	const bothViews = { combined: combinedTree, grouped: groupedTreeWithEmptyCategory };

	it('offers one control per configured category, including one with no active work', async () => {
		await renderGrouped({ trees: bothViews });
		const filter = await openCategories();
		// `docs` is configured but holds no active work, so it has no group…
		expect(screen.queryByTestId('group-docs')).toBeNull();
		// …yet it must still be offered, because selection is about categories,
		// not about what happens to be busy today.
		const options = filter.querySelectorAll('[data-testid^="category-option-"]');
		expect(Array.from(options).map((option) => option.getAttribute('data-testid'))).toEqual([
			'category-option-bug',
			'category-option-docs',
			'category-option-task'
		]);
	});

	it('carries each category’s shape and name, and states its selected state in words', async () => {
		await renderGrouped({ trees: bothViews });
		await openCategories();
		const option = screen.getByTestId('category-option-docs');
		const shape = option.querySelector('[data-shape]');
		expect(shape?.getAttribute('data-shape')).toBe('triangle');
		expect(shape?.getAttribute('aria-label')).toBe('Docs category');
		expect(option.textContent).toContain('Docs');
		// Every category starts selected, and the accessible name says so: the
		// state is never carried by colour or by `aria-pressed` alone.
		expect(await optionLabel('docs')).toBe('Docs, selected');
		expect(await pressed('docs')).toBe('true');
		expect(screen.getByRole('dialog', { name: 'Categories' })).not.toBeNull();
	});

	it('filters to one category, then to several, keeping a required ancestor as context', async () => {
		await renderGrouped({ trees: bothViews });
		// Off: the category's group and its work items leave.
		await clickCategory('task');
		expect(screen.queryByTestId('group-task')).toBeNull();
		expect(screen.getByTestId('group-bug')).not.toBeNull();
		expect(await optionLabel('task')).toBe('Tasks, not selected');
		expect(await pressed('task')).toBe('false');

		// Back on, then everything except `task` turned off: only `task` is left.
		await clickCategory('task');
		await clickCategory('bug');
		await clickCategory('docs');
		// The bug group stays as context: `wm-task-child`'s real parent is a bug,
		// and promoting the task to a root would falsify its parentage.
		expect(screen.queryByTestId('group-docs')).toBeNull();
		expect(screen.getByTestId('group-task')).not.toBeNull();
		expect(screen.getByTestId('group-bug')).not.toBeNull();
		expect(screen.getByTestId('node-wm-task-ship')).not.toBeNull();
		expect(screen.getByTestId('node-wm-task-child')).not.toBeNull();
		expect(screen.getByTestId('node-wm-bug-login').getAttribute('data-context')).toBe('true');
		// The context ancestor is not selected work: two tasks are counted, and
		// the blocked bug above them does not inflate the blocked count.
		expect(screen.getByTestId('tree-summary').textContent).toBe('2 items · 0 blocked');

		// Two categories selected at once: work items from both are shown.
		await clickCategory('bug');
		expect(await pressed('bug')).toBe('true');
		expect(await pressed('task')).toBe('true');
		expect(screen.getByTestId('group-bug')).not.toBeNull();
		expect(screen.getByTestId('group-task')).not.toBeNull();
		expect(screen.getByTestId('node-wm-task-ship')).not.toBeNull();
		expect(screen.getByTestId('node-wm-bug-login').getAttribute('data-context')).toBeNull();
		expect(screen.getByTestId('tree-summary').textContent).toBe('3 items · 1 blocked');
	});

	it('keeps a whole unselected ancestor chain as context without its siblings', async () => {
		await renderLoaded({ trees: { combined: combinedTree, grouped: deepGroupedTree } });
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('group-docs');
		await clickCategory('bug');
		await clickCategory('docs');

		// Docs and Bugs are out of scope; both survive only as the chain down to
		// the task, and their unrelated children do not come along.
		expect(screen.getByTestId('node-wm-docs-umbrella').getAttribute('data-context')).toBe('true');
		expect(screen.getByTestId('node-wm-bug-mid').getAttribute('data-context')).toBe('true');
		expect(screen.getByTestId('node-wm-task-leaf')).not.toBeNull();
		expect(screen.queryByTestId('node-wm-docs-sibling')).toBeNull();
		expect(screen.queryByTestId('node-wm-bug-sibling')).toBeNull();
		expect(screen.getByTestId('tree-summary').textContent).toBe('2 items · 0 blocked');

		// A selected context ancestor collapses like any other node, and its kept
		// descendant goes with it until it is expanded again.
		await fireEvent.click(screen.getByTestId('toggle-wm-docs-umbrella'));
		expect(screen.queryByTestId('node-wm-bug-mid')).toBeNull();
		await fireEvent.click(screen.getByTestId('toggle-wm-docs-umbrella'));
		expect(screen.getByTestId('node-wm-bug-mid')).not.toBeNull();
	});

	it('keeps context rows out of the unresolved-relations area', async () => {
		// A genuine parent problem (an item whose parent is missing) is reported
		// there; a context ancestor is only outside the selection, not broken.
		const withUnresolved = {
			...deepGroupedTree,
			unresolved: [
				{
					id: 'wm-orphan',
					title: 'Orphaned work',
					category: TASK,
					parent: 'wm-missing',
					reason: 'parent_missing' as const
				}
			]
		};
		await renderLoaded({ trees: { combined: combinedTree, grouped: withUnresolved } });
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('group-docs');
		await clickCategory('bug');
		await clickCategory('docs');

		expect(screen.getByTestId('node-wm-docs-umbrella').getAttribute('data-context')).toBe('true');
		const unresolved = screen.getByTestId('unresolved');
		expect(unresolved.textContent).toContain('Orphaned work');
		// The context ancestors are inside the tree, not listed as problems.
		expect(unresolved.textContent).not.toContain('Documentation umbrella');
		expect(unresolved.textContent).not.toContain('Middle bug');
		expect(screen.getByTestId('tree-summary').textContent).toBe('2 items · 0 blocked · 1 unresolved');
	});

	it('marks no row in the combined view, which filters nothing', async () => {
		localStorage.setItem(
			preferenceKey(projectIdFor(ROOT_A), 'categories'),
			JSON.stringify(['task'])
		);
		const api = makeFetch({ trees: bothViews });
		const { container } = render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
		await screen.findByTestId('node-wm-bug-login');
		expect(container.querySelectorAll('[data-context]')).toHaveLength(0);
		expect(container.querySelectorAll('[data-testid^="context-marker-"]')).toHaveLength(0);
	});

	it('selects all and clears all with real buttons', async () => {
		await renderGrouped({ trees: bothViews });
		await clickCategory('bug');
		await clickCategory('task');
		await clickCategory('docs');
		expect(screen.queryByTestId('group-bug')).toBeNull();
		expect(screen.queryByTestId('group-task')).toBeNull();
		expect(screen.getByTestId('filtered-empty-state')).not.toBeNull();

		// The toolbar's control, not the recovery action the empty pane repeats.
		await clickCategoryAction('Select all categories');
		expect(screen.getByTestId('group-bug')).not.toBeNull();
		expect(screen.getByTestId('group-task')).not.toBeNull();
		expect(await pressed('docs')).toBe('true');
		expect(screen.queryByTestId('filtered-empty-state')).toBeNull();

		await clickCategoryAction('Clear all categories');
		expect(screen.queryByTestId('group-bug')).toBeNull();
		expect(screen.queryByTestId('group-task')).toBeNull();
		expect(await pressed('bug')).toBe('false');
		expect(await pressed('task')).toBe('false');
	});

	it('leaves the combined view alone, whatever the stored grouped scope says', async () => {
		// A stored scope that hides two categories, written by an earlier session.
		localStorage.setItem(preferenceKey(projectIdFor(ROOT_A), 'categories'), JSON.stringify(['bug']));

		const api = makeFetch({ trees: bothViews });
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
		await screen.findByTestId('node-wm-bug-login');
		// The combined view is one hierarchy across categories: no control, no filter.
		expect(screen.queryByTestId('category-filter')).toBeNull();
		expect(screen.getByTestId('node-wm-task-ship')).not.toBeNull();
		expect(screen.getByTestId('node-wm-task-child')).not.toBeNull();
		expect(screen.getByTestId('tree-summary').textContent).toBe('3 items · 1 blocked');
	});

	it('ignores a stored category this project no longer configures', async () => {
		localStorage.setItem(
			preferenceKey(projectIdFor(ROOT_A), 'categories'),
			JSON.stringify(['bug', 'obsolete'])
		);

		await renderGrouped({ trees: bothViews });
		expect(screen.getByTestId('group-bug')).not.toBeNull();
		expect(screen.queryByTestId('group-task')).toBeNull();
		expect(await pressed('bug')).toBe('true');
		expect(await pressed('task')).toBe('false');
		expect(screen.queryByTestId('category-option-obsolete')).toBeNull();
	});

	it('remembers the scope per project across a reload', async () => {
		const first = await renderGrouped({ trees: bothViews });
		await clickCategory('bug');
		// The bug group stays as context for the task nested under it, but no bug
		// is selected work any more.
		expect(screen.getByTestId('node-wm-bug-login').getAttribute('data-context')).toBe('true');
		first.unmount();

		const second = makeFetch({ trees: bothViews });
		render(Explorer, { fetchImpl: second.fetchImpl, storage: localStorage });
		await screen.findByTestId('group-task');
		// The stored view is `grouped`, so the scope is visible straight away.
		expect(await pressed('bug')).toBe('false');
		expect(await pressed('task')).toBe('true');
		expect(await pressed('docs')).toBe('true');
		expect(screen.getByTestId('node-wm-bug-login').getAttribute('data-context')).toBe('true');
		expect(
			localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'categories'))
		).toBe(JSON.stringify(['docs', 'task']));
	});

	it('does not hand one project’s scope to another on the same origin', async () => {
		const first = await renderGrouped({ root: ROOT_A, trees: bothViews });
		await clickCategory('bug');
		await clickCategory('docs');
		first.unmount();

		const second = makeFetch({ root: ROOT_B, trees: bothViews });
		render(Explorer, { fetchImpl: second.fetchImpl, storage: localStorage });
		await screen.findByTestId('node-wm-bug-login');
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('group-bug');
		expect(await pressed('bug')).toBe('true');
		expect(await pressed('task')).toBe('true');
		expect(screen.getByTestId('group-task')).not.toBeNull();
	});

	it('filters for the session and keeps the one persistence notice when storage is unavailable', async () => {
		const api = makeFetch({ trees: bothViews });
		render(Explorer, { fetchImpl: api.fetchImpl, storage: null });
		await screen.findByTestId('node-wm-bug-login');
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('group-bug');

		await clickCategory('task');
		expect(screen.queryByTestId('group-task')).toBeNull();
		expect(screen.getByTestId('group-bug')).not.toBeNull();
		await clickCategoryAction('Select all categories');
		expect(screen.getByTestId('group-task')).not.toBeNull();

		// Still exactly one notice, even after the scope writes failed.
		expect(screen.getAllByTestId('persistence-notice')).toHaveLength(1);
		expect(screen.getByTestId('persistence-notice').textContent).toContain(
			'Preferences could not be saved'
		);
	});
});

describe('status section', () => {
	it('shows the status counts and no banner for a healthy projection', async () => {
		const api = makeFetch({ trees: { combined: combinedTree }, status: healthyStatus });
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
		await screen.findByTestId('node-wm-bug-login');
		await screen.findByTestId('status-completeness');

		expect(api.urls).toContain('/api/status');
		expect(screen.getByTestId('status-completeness').textContent).toBe('Complete');
		expect(screen.getByTestId('status-freshness').textContent).toBe('Fresh');
		expect(screen.getByTestId('status-cache-total').textContent).toBe('0');
		expect(screen.getByTestId('status-cache-by-class').textContent).toBe('0 / 0 / 0');
		expect(screen.getByTestId('status-counters').textContent).toBe('0 / 0 / 0');
		expect(screen.getByTestId('status-cache-enabled').textContent).toBe('enabled');
		expect(screen.getByTestId('status-cache-capacity').textContent).toBe('512');
		// A complete, fresh projection must not raise the banner.
		expect(screen.queryByTestId('status-banner')).toBeNull();
	});

	it('shows the prominent banner with diagnostics for a stale projection', async () => {
		const api = makeFetch({ trees: { combined: combinedTree }, status: staleStatus });
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
		await screen.findByTestId('node-wm-bug-login');
		await screen.findByTestId('status-banner');

		const banner = screen.getByTestId('status-banner');
		expect(banner.getAttribute('role')).toBe('alert');
		expect(banner.textContent).toContain('stale');
		expect(screen.getByTestId('status-banner-counts').textContent).toContain('1 document(s)');
		expect(screen.getByTestId('status-banner-diagnostics').textContent).toContain(
			'work/bugs/wm-broken.md'
		);
		expect(screen.getByTestId('status-freshness').textContent).toBe('Stale');
	});

	it('shows the banner for a never-loaded projection', async () => {
		const api = makeFetch({ trees: { combined: combinedTree }, status: neverLoadedStatus });
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
		await screen.findByTestId('node-wm-bug-login');
		await screen.findByTestId('status-banner');

		const banner = screen.getByTestId('status-banner');
		expect(banner.getAttribute('role')).toBe('alert');
		expect(banner.textContent).toContain('unknown');
		expect(screen.getByTestId('status-last-success').textContent).toBe('never loaded');
		expect(screen.getByTestId('status-completeness').textContent).toBe('Incomplete');
	});

	it('shows the banner when the projection is merely incomplete', async () => {
		const incomplete = makeFetch({
			trees: { combined: combinedTree },
			status: {
				...healthyStatus,
				projection: {
					...healthyStatus.projection,
					completeness: 'scoped',
					freshness: 'fresh',
					unreadable_documents: 2,
					uncertain_paths: 0,
					diagnostics: [incompleteProjectionSummary({ documents: 2 })]
				}
			}
		});
		render(Explorer, { fetchImpl: incomplete.fetchImpl, storage: localStorage });
		await screen.findByTestId('node-wm-bug-login');
		await screen.findByTestId('status-banner');

		const banner = screen.getByTestId('status-banner');
		expect(banner.getAttribute('role')).toBe('alert');
		expect(banner.textContent).toContain('incomplete');
		expect(screen.getByTestId('status-banner-counts').textContent).toContain('2 document(s)');
		// The summary finding is not a document cause, so it is not listed as one.
		expect(screen.queryByTestId('status-banner-diagnostics')).toBeNull();
	});

	it('does not fail the page when the status read itself fails', async () => {
		const api = makeFetch({
			trees: { combined: combinedTree },
			statusFailure: failure('NETWORK_ERROR', 'the Web service could not be reached')
		});
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });

		// The tree still loads and renders, so the page is not failed.
		await screen.findByTestId('node-wm-bug-login');
		const statusFailure = await screen.findByTestId('status-failure');
		expect(statusFailure.textContent).toContain('NETWORK_ERROR');
		expect(statusFailure.textContent).toContain('tree view still works');
		expect(screen.queryByTestId('status-banner')).toBeNull();
	});

	it('loads status exactly once and never polls', async () => {
		const api = makeFetch({ trees: { combined: combinedTree, grouped: groupedTree } });
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
		await screen.findByTestId('node-wm-bug-login');
		// Wait until the status read has settled so the count is final.
		await screen.findByTestId('status-completeness');
		const statusReads = api.urls.filter((url) => url === '/api/status');
		expect(statusReads).toHaveLength(1);

		// Interacting with the tree must not trigger another status read.
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('group-bug');
		expect(api.urls.filter((url) => url === '/api/status')).toHaveLength(1);
		expect(screen.queryByTestId('status-loading')).toBeNull();
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
