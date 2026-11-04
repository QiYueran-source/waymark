import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

// The real stylesheet: the centred/inline split and the scrollable diagnostics
// block are layout contracts a reader sees, so assert them against the rules
// that ship rather than against a class name alone.
import '../src/app.css';
import Explorer from '$lib/components/Explorer.svelte';
import DetailPanel from '$lib/components/DetailPanel.svelte';
import {
	combinedTree,
	detail,
	emptyTree,
	failure,
	groupedTreeWithEmptyCategory,
	makeFetch,
	unresolvedTree
} from './fixtures';
import { clickCategory, clickCategoryAction } from './category-helpers';

type FetchOptions = Parameters<typeof makeFetch>[0];

/** The one layout marker for a blocking or empty state. */
const CENTRED = '[data-state-layout="centered"]';

async function renderLoaded(options: FetchOptions) {
	const api = makeFetch(options);
	const view = render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
	await screen.findByTestId('tree-summary');
	return { ...api, ...view };
}

async function openGrouped(options: FetchOptions) {
	const rendered = await renderLoaded(options);
	await fireEvent.click(screen.getByLabelText('By category'));
	await screen.findByTestId('group-bug');
	return rendered;
}

/** The element is the centred state itself, not merely inside one. */
function expectCentred(element: HTMLElement): void {
	expect(element.getAttribute('data-state-layout')).toBe('centered');
	const style = getComputedStyle(element);
	expect(style.display).toBe('flex');
	expect(style.alignItems).toBe('center');
	expect(style.justifyContent).toBe('center');
	// It fills the content area the pane has, rather than a fixed height: the
	// scrolling tree pane is a block (`min-height`), the graph pane and detail
	// panel are flex columns (`flex-grow`).
	expect(style.minHeight === '100%' || style.flexGrow === '1').toBe(true);
	expect(style.height).not.toMatch(/^\d+px$/);
}

/** The element is wrapped by a centred state; return that wrapper. */
function centredHost(element: Element | null): HTMLElement {
	const host = element?.closest(CENTRED) as HTMLElement | null;
	expect(host).not.toBeNull();
	return host as HTMLElement;
}

describe('blocking and empty states are centred', () => {
	it('centres the no-category-selected state, with its recovery action', async () => {
		await openGrouped({ trees: { combined: combinedTree, grouped: groupedTreeWithEmptyCategory } });
		await clickCategoryAction('Clear all categories');

		const empty = await screen.findByTestId('filtered-empty-state');
		expect(empty.getAttribute('data-state-kind')).toBe('categories-cleared');
		expect(empty.getAttribute('data-empty-kind')).toBe('categories-cleared');
		expectCentred(empty);
		expect(empty.textContent).toContain('No categories selected');
		// The recovery action remains inside the state a reader is looking at.
		expect(within(empty).getByTestId('select-all-from-empty')).not.toBeNull();
		// It is not an ordinary content row: no tree lives behind it.
		expect(screen.queryByTestId('node-wm-bug-login')).toBeNull();
	});

	it('centres the selected-categories-with-no-active-work state', async () => {
		await openGrouped({ trees: { combined: combinedTree, grouped: groupedTreeWithEmptyCategory } });
		// Docs is configured but holds no active work: select only it.
		await clickCategory('bug');
		await clickCategory('task');

		const empty = await screen.findByTestId('filtered-empty-state');
		expect(empty.getAttribute('data-state-kind')).toBe('no-active-work');
		expect(empty.getAttribute('data-empty-kind')).toBe('no-active-work');
		expectCentred(empty);
		expect(empty.textContent).toContain('Nothing in the selected categories');
	});

	it('centres the empty-project state', async () => {
		const api = makeFetch({ trees: { combined: emptyTree } });
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });

		const empty = await screen.findByTestId('empty-state');
		expect(empty.getAttribute('data-state-kind')).toBe('empty-project');
		// Only the two filtered-empty kinds carry the older hook.
		expect(empty.getAttribute('data-empty-kind')).toBeNull();
		expectCentred(empty);
		expect(empty.textContent).toContain('No active work items');
	});

	it('centres a project read failure and wraps its diagnostics', async () => {
		const api = makeFetch({
			treeFailure: failure('INCOMPLETE_PROJECTION', 'the project projection is incomplete', [
				'work/tasks/wm-broken.md: damaged document'
			])
		});
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });

		const alert = await screen.findByTestId('read-failure');
		const host = centredHost(alert);
		expect(host.getAttribute('data-state-kind')).toBe('read-failure');
		expect(host.getAttribute('data-empty-kind')).toBeNull();
		expectCentred(host);
		expect(host.closest('.tree-pane')).not.toBeNull();
		expect(alert.getAttribute('role')).toBe('alert');
		// The content is inside the centred state, not duplicated beside it.
		expect(host.textContent).toContain('INCOMPLETE_PROJECTION');
		expect(host.textContent).toContain('work/tasks/wm-broken.md');
	});

	it('centres a detail-load failure inside the detail panel', async () => {
		const api = makeFetch({
			trees: { combined: combinedTree },
			itemFailure: failure('ITEM_NOT_FOUND', 'no item with that id')
		});
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
		await screen.findByTestId('node-wm-bug-login');
		await fireEvent.click(screen.getByTestId('select-wm-bug-login'));

		const alert = await screen.findByTestId('detail-failure');
		const host = centredHost(alert);
		expect(host.getAttribute('data-state-kind')).toBe('detail-failure');
		expect(host.getAttribute('data-empty-kind')).toBeNull();
		expectCentred(host);
		// Centred in the panel's own remaining area, not in the tree pane.
		expect(screen.getByTestId('detail-panel').contains(host)).toBe(true);
		expect(screen.getByTestId('detail-panel').contains(alert)).toBe(true);
		expect(screen.getByLabelText('Work items').contains(host)).toBe(false);
	});

	it('does not centre an ordinary detail panel or its loading status', async () => {
		const onClose = () => {};
		const onSelect = () => {};
		render(DetailPanel, { id: detail.id, detail, failure: null, loading: false, onClose, onSelect });
		expect(screen.getByTestId('detail-panel').querySelector(CENTRED)).toBeNull();
	});

	it('does not centre the loading status of a blocking read', () => {
		// A deferred tree read keeps the loading status on screen.
		const never = new Promise<Response>(() => {});
		const fetchImpl = (() => never) as unknown as typeof fetch;
		render(Explorer, { fetchImpl, storage: localStorage });
		const status = screen.getByText('Reading the project tree…');
		expect(status.closest(CENTRED)).toBeNull();
	});
});

describe('non-blocking notices stay inline at the top', () => {
	it('keeps the persistence notice inline, outside the centred empty state', async () => {
		const api = makeFetch({ trees: { combined: emptyTree } });
		render(Explorer, { fetchImpl: api.fetchImpl, storage: null });

		const empty = await screen.findByTestId('empty-state');
		const notice = screen.getByTestId('persistence-notice');
		expect(notice.getAttribute('data-state-layout')).toBe('inline');
		expect(notice.closest(CENTRED)).toBeNull();
		expect(empty.contains(notice)).toBe(false);

		// At the top: ahead of the toolbar and the content layout, not floating
		// over the pane it would otherwise cover.
		const layout = document.querySelector('.layout') as HTMLElement;
		const toolbar = document.querySelector('.toolbar') as HTMLElement;
		expect(notice.compareDocumentPosition(toolbar) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
		expect(notice.compareDocumentPosition(layout) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
		expect(layout.contains(notice)).toBe(false);
	});
});

describe('long diagnostics', () => {
	it('uses a centred, width-limited, scrollable block that wraps long lines', async () => {
		const longDiagnostic = `work/tasks/wm-broken.md: ${'damaged-project-document '.repeat(40)}`;
		const api = makeFetch({
			treeFailure: failure('INCOMPLETE_PROJECTION', 'the project projection is incomplete', [
				longDiagnostic
			])
		});
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });

		const block = await screen.findByTestId('read-failure-diagnostics-block');
		expect(block.getAttribute('data-scrollable')).toBe('true');
		centredHost(block);
		expect(block.textContent).toContain(longDiagnostic);

		const style = getComputedStyle(block);
		expect(style.overflowY).toBe('auto');
		expect(style.maxHeight).not.toBe('');
		expect(style.maxHeight).not.toBe('none');
		expect(style.whiteSpace).toBe('normal');
		expect(style.overflowWrap).toBe('anywhere');

		// Width comes from the bounded failure card, not from the line's length.
		const failureStyle = getComputedStyle(screen.getByTestId('read-failure'));
		expect(failureStyle.maxWidth).not.toBe('');
		expect(failureStyle.maxWidth).not.toBe('none');
	});
});

describe('unresolved relations are content, not a message', () => {
	it('keeps the unresolved list in its own area, outside every message layout', async () => {
		await renderLoaded({ trees: { combined: unresolvedTree } });

		const unresolved = screen.getByTestId('unresolved');
		expect(unresolved.getAttribute('data-state-layout')).toBeNull();
		expect(unresolved.closest(CENTRED)).toBeNull();
		expect(unresolved.closest('[data-state-layout]')).toBeNull();
		// It stays content in the pane, below the hierarchy it belongs to.
		expect(unresolved.closest('.tree-pane')).not.toBeNull();
		expect(unresolved.textContent).toContain('Orphaned work');
	});
});
