/**
 * The graph's reading direction, end to end.
 *
 * The drawing is the same hierarchy in one of two orientations: top to bottom
 * (the first-use default) or left to right. These tests drive the rendered
 * Explorer, so what they assert is what a reader gets: where the control is,
 * where a card sits relative to its parent, what survives a switch, and what a
 * reload restores.
 *
 * Geometry is read from the model's own hooks (`data-graph-x`, `data-graph-y`,
 * and the edge handle names) rather than from the drawing library's internals,
 * so the contract does not depend on any particular library API.
 */
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import Explorer from '$lib/components/Explorer.svelte';
import { preferenceKey, projectIdFor } from '$lib/storage';
import {
	BUG,
	ROOT_A,
	ROOT_B,
	TASK,
	combinedTree,
	detail,
	groupedTreeWithEmptyCategory,
	hierarchyTree,
	makeFetch,
	multiDependencyTree,
	treeNode
} from './fixtures';
import { clickCategory } from './category-helpers';
import type { CombinedTree } from '$lib/types';

type FetchOptions = Parameters<typeof makeFetch>[0];

const BOTH_VIEWS = { combined: combinedTree, grouped: groupedTreeWithEmptyCategory };

async function renderLoaded(options: FetchOptions, storage: Storage | null = localStorage) {
	const api = makeFetch(options);
	const view = render(Explorer, { fetchImpl: api.fetchImpl, storage });
	await screen.findByTestId('tree-summary');
	return { ...api, ...view };
}

async function openGraph(options: FetchOptions, storage: Storage | null = localStorage) {
	const rendered = await renderLoaded(options, storage);
	await fireEvent.click(screen.getByLabelText('Graph'));
	await screen.findByTestId('graph');
	await waitFor(() => expect(cards().length).toBeGreaterThan(0));
	return rendered;
}

/** Render a grouped project and open its graph, with the category picker present. */
async function openGroupedGraph(options: FetchOptions = { trees: BOTH_VIEWS }) {
	const rendered = await renderLoaded(options);
	await fireEvent.click(screen.getByLabelText('By category'));
	await screen.findByTestId('group-bug');
	await fireEvent.click(screen.getByLabelText('Graph'));
	await screen.findByTestId('graph');
	await waitFor(() => expect(cards().length).toBeGreaterThan(0));
	return rendered;
}

/** The graph's node cards, in document order. */
function cards(): HTMLElement[] {
	return screen.getAllByTestId(/^graph-node-/);
}

function card(id: string): HTMLElement {
	return screen.getByTestId(`graph-node-${id}`);
}

function drawnIds(): string[] {
	return cards().map((element) => element.getAttribute('data-node-id') ?? '');
}

function number(element: HTMLElement, attribute: string): number {
	const raw = element.getAttribute(attribute);
	expect(raw).not.toBeNull();
	return Number(raw);
}

function directionControl(): HTMLElement {
	return screen.getByTestId('graph-direction');
}

function directionRadio(name: 'Top to bottom' | 'Left to right'): HTMLInputElement {
	return within(directionControl()).getByRole('radio', { name }) as HTMLInputElement;
}

function checkedDirection(): string {
	if (directionRadio('Left to right').checked) return 'LR';
	if (directionRadio('Top to bottom').checked) return 'TB';
	return 'none';
}

async function switchTo(name: 'Top to bottom' | 'Left to right'): Promise<void> {
	await fireEvent.click(directionRadio(name));
	await waitFor(() => expect(checkedDirection()).toBe(name === 'Left to right' ? 'LR' : 'TB'));
}

function viewport() {
	const element = screen.getByTestId('graph-viewport');
	const read = (name: string) => Number(element.getAttribute(name));
	return {
		x: read('data-x'),
		y: read('data-y'),
		zoom: read('data-zoom'),
		fitted: element.getAttribute('data-fitted') === 'true'
	};
}

function pane(): HTMLElement {
	return screen.getByTestId('graph').querySelector('.svelte-flow__pane') as HTMLElement;
}

/** Drag the background, the gesture a reader pans the canvas with. */
async function dragBackground(from: [number, number], to: [number, number]) {
	const event = (type: string, [x, y]: [number, number]) => {
		const built = new MouseEvent(type, {
			bubbles: true,
			cancelable: true,
			button: 0,
			clientX: x,
			clientY: y
		});
		Object.defineProperty(built, 'view', { value: window });
		return built;
	};
	pane().dispatchEvent(event('mousedown', from));
	window.dispatchEvent(event('mousemove', to));
	window.dispatchEvent(event('mouseup', to));
	await waitFor(() => expect(viewport().fitted).toBe(false));
	await new Promise((resolve) => setTimeout(resolve, 0));
}

/** An edge of one kind, found by its relation. */
function edge(
	kind: 'parent-child' | 'dependency',
	from: string,
	to: string
): HTMLElement | undefined {
	return screen
		.getAllByTestId('graph-edge')
		.find(
			(element) =>
				element.getAttribute('data-edge-kind') === kind &&
				element.getAttribute('data-from') === from &&
				element.getAttribute('data-to') === to
		);
}

/** A parent with two children, for sibling order and stacking. */
const fanFirst = treeNode({
	id: 'wm-fan-first',
	title: 'First child',
	category: TASK,
	parent: 'wm-fan-root'
});

const fanSecond = treeNode({
	id: 'wm-fan-second',
	title: 'Second child',
	category: TASK,
	parent: 'wm-fan-root'
});

const fanRoot = treeNode({
	id: 'wm-fan-root',
	title: 'Fan root',
	category: BUG,
	children: [fanFirst, fanSecond]
});

const fanTree: CombinedTree = {
	view: 'combined',
	roots: [fanRoot],
	unresolved: [],
	categories: [BUG, TASK]
};

describe('the direction control', () => {
	it('offers top to bottom and left to right only in graph presentation', async () => {
		await renderLoaded({ trees: BOTH_VIEWS });

		// The tree cannot be affected by a graph direction, so it shows no
		// control at all — not merely a disabled one.
		expect(screen.queryByTestId('graph-direction')).toBeNull();
		expect(screen.queryByRole('radio', { name: 'Top to bottom' })).toBeNull();
		expect(screen.queryByRole('radio', { name: 'Left to right' })).toBeNull();

		// Grouped mode has the category picker, so its graph is where the whole
		// upper-left cluster can be checked at once.
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('group-bug');
		expect(screen.queryByTestId('graph-direction')).toBeNull();

		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');

		// One segmented control in the existing upper-left graph cluster, beside
		// the canvas row; grouped mode also puts the category picker there.
		const cluster = screen.getByTestId('graph-controls-cluster');
		const control = directionControl();
		expect(cluster.contains(control)).toBe(true);
		expect(control.closest('.graph-controls-row')).toBeNull();
		expect(cluster.querySelector('.graph-controls-row')).not.toBeNull();
		expect(cluster.querySelector('[data-testid="category-filter"]')).not.toBeNull();

		// First use is top to bottom, stated in words rather than by an icon.
		expect(directionRadio('Top to bottom').checked).toBe(true);
		expect(directionRadio('Left to right').checked).toBe(false);
		expect(screen.getByTestId('graph').getAttribute('data-direction')).toBe('TB');

		await fireEvent.click(screen.getByLabelText('Tree'));
		await screen.findByTestId('node-wm-bug-login');
		expect(screen.queryByTestId('graph-direction')).toBeNull();
	});
});

/** A child that depends on an unrelated root: a dependency across generations. */
const crossSource = treeNode({
	id: 'wm-cross-source',
	title: 'Cross source',
	category: TASK,
	parent: 'wm-cross-root',
	dependencies: ['wm-cross-target']
});

const crossRoot = treeNode({
	id: 'wm-cross-root',
	title: 'Cross root',
	category: TASK,
	children: [crossSource]
});

const crossTarget = treeNode({ id: 'wm-cross-target', title: 'Cross target', category: BUG });

const crossTree: CombinedTree = {
	view: 'combined',
	roots: [crossRoot, crossTarget],
	unresolved: [],
	categories: [BUG, TASK]
};

describe('the left-to-right layout', () => {
	it('puts parents left of children and stacks siblings vertically', async () => {
		await openGraph({ trees: { combined: fanTree } });

		// The default is still the directory-style top-to-bottom layout, with
		// the siblings side by side under their parent.
		expect(number(card('wm-fan-first'), 'data-graph-y')).toBeGreaterThan(
			number(card('wm-fan-root'), 'data-graph-y')
		);
		const siblingOrder = (axis: 'data-graph-x' | 'data-graph-y'): string[] =>
			['wm-fan-first', 'wm-fan-second'].sort(
				(left, right) => number(card(left), axis) - number(card(right), axis)
			);
		const rowOrder = siblingOrder('data-graph-x');
		expect(rowOrder).toHaveLength(2);

		await switchTo('Left to right');

		// Each generation is a column, and a child sits to the right of its
		// parent rather than below it.
		expect(number(card('wm-fan-first'), 'data-graph-x')).toBeGreaterThan(
			number(card('wm-fan-root'), 'data-graph-x')
		);
		expect(number(card('wm-fan-second'), 'data-graph-x')).toBe(
			number(card('wm-fan-first'), 'data-graph-x')
		);
		expect(number(card('wm-fan-first'), 'data-graph-y')).not.toBe(
			number(card('wm-fan-second'), 'data-graph-y')
		);
		// The same siblings keep their relative order: the one that was left in
		// the row is the one that is above in the column. Only the axis changed.
		expect(siblingOrder('data-graph-y')).toEqual(rowOrder);
	});

	it('leaves a parent’s right side and enters a child’s left side', async () => {
		await openGraph({ trees: { combined: fanTree } });

		// Top to bottom keeps the existing bottom-to-top handles.
		const vertical = edge('parent-child', 'wm-fan-root', 'wm-fan-first');
		expect(vertical).not.toBeUndefined();
		expect(vertical?.getAttribute('data-source-handle')).toBe('out-bottom');
		expect(vertical?.getAttribute('data-target-handle')).toBe('in-top');

		await switchTo('Left to right');

		const horizontal = edge('parent-child', 'wm-fan-root', 'wm-fan-first');
		expect(horizontal?.getAttribute('data-source-handle')).toBe('out-right');
		expect(horizontal?.getAttribute('data-target-handle')).toBe('in-left');
		// Same relation, same direction: only the sides changed.
		expect(horizontal?.getAttribute('data-from')).toBe('wm-fan-root');
		expect(horizontal?.getAttribute('data-to')).toBe('wm-fan-first');
	});

	it('returns to the top-to-bottom geometry when switched back', async () => {
		await openGraph({ trees: { combined: fanTree } });
		const entry = viewport();

		await switchTo('Left to right');
		await switchTo('Top to bottom');

		expect(screen.getByTestId('graph').getAttribute('data-direction')).toBe('TB');
		expect(number(card('wm-fan-first'), 'data-graph-y')).toBeGreaterThan(
			number(card('wm-fan-root'), 'data-graph-y')
		);
		const vertical = edge('parent-child', 'wm-fan-root', 'wm-fan-first');
		expect(vertical?.getAttribute('data-source-handle')).toBe('out-bottom');
		expect(vertical?.getAttribute('data-target-handle')).toBe('in-top');
		// The drawing is fitted again for the restored orientation.
		await waitFor(() => expect(viewport().fitted).toBe(true));
		expect(entry.fitted).toBe(true);
	});
});

describe('dependency routing in left to right', () => {
	it('routes a same-generation dependency along the column, not across it', async () => {
		await openGraph({ trees: { combined: combinedTree } });
		// Both roots are generation 0, so in left-to-right they share one column.
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		const dependency = () => edge('dependency', 'wm-task-ship', 'wm-bug-login');
		await waitFor(() => expect(dependency()).not.toBeUndefined());

		await switchTo('Left to right');

		// The two cards are stacked, so the dependency leaves the top or bottom
		// side of the source and enters the facing side of the target.
		const handles = [
			dependency()?.getAttribute('data-source-handle') ?? '',
			dependency()?.getAttribute('data-target-handle') ?? ''
		];
		expect(handles[0]).toMatch(/^out-(top|bottom)/);
		expect(handles[1]).toMatch(/^in-(bottom|top)/);
		// The relation's own direction is unchanged: it still points at the
		// depended-on item, and the accessible name still says which way.
		expect(dependency()?.getAttribute('data-from')).toBe('wm-task-ship');
		expect(dependency()?.getAttribute('data-to')).toBe('wm-bug-login');
		expect(dependency()?.getAttribute('data-arrow')).toBe('target');
	});

	it('describes a shared same-generation corridor as a column, not a row', async () => {
		await openGraph({ trees: { combined: multiDependencyTree } });
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		await waitFor(() =>
			expect(screen.getAllByTestId('graph-gap-count').length).toBeGreaterThan(0)
		);

		await switchTo('Left to right');
		const marker = await waitFor(() => {
			const found = screen
				.getAllByTestId('graph-gap-count')
				.find((element) => element.getAttribute('data-gap') === 'row:1');
			expect(found).not.toBeUndefined();
			return found as HTMLElement;
		});
		// The corridor is the same relation on the same generation, but in this
		// orientation it is a column; naming it a row would describe the wrong
		// geometry.
		expect(marker.textContent).toContain('generation 1 column');
		expect(marker.textContent).not.toContain('row');
	});

	it('routes a cross-generation dependency through the side facing its target', async () => {
		await openGraph({ trees: { combined: crossTree } });
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		const dependency = () => edge('dependency', 'wm-cross-source', 'wm-cross-target');
		await waitFor(() => expect(dependency()).not.toBeUndefined());

		await switchTo('Left to right');

		// The dependent child is one column to the right of its target, so the
		// line leaves the child's left side and enters the target's right.
		expect(dependency()?.getAttribute('data-source-handle')).toBe('out-left');
		expect(dependency()?.getAttribute('data-target-handle')).toBe('in-right');
		expect(dependency()?.getAttribute('data-arrow')).toBe('target');

		// A cross-generation dependency continues to use the top/bottom sides in
		// the default orientation.
		await switchTo('Top to bottom');
		expect(dependency()?.getAttribute('data-source-handle')).toBe('out-top');
		expect(dependency()?.getAttribute('data-target-handle')).toBe('in-bottom');
	});
});

/** A node wrapper's placed position and size, as the renderer wrote them. */
function parseTransform(wrapper: HTMLElement): {
	transform: { x: number; y: number };
	width: number;
	height: number;
} {
	const style = wrapper.getAttribute('style') ?? '';
	const translate = /translate\((-?[\d.]+)px,\s*(-?[\d.]+)px\)/.exec(style);
	const width = /width:\s*([\d.]+)px/.exec(style);
	const height = /height:\s*([\d.]+)px/.exec(style);
	expect(translate).not.toBeNull();
	return {
		transform: { x: Number(translate?.[1] ?? 0), y: Number(translate?.[2] ?? 0) },
		width: Number(width?.[1] ?? 0),
		height: Number(height?.[1] ?? 0)
	};
}

/** The drawn area, in canvas pixels, as the reader sees it now. */
function drawnBounds() {
	const shown = viewport();
	const boxes = cards().map((element) => {
		const wrapper = element.closest('.svelte-flow__node') as HTMLElement;
		const { transform, width, height } = parseTransform(wrapper);
		return {
			left: transform.x * shown.zoom + shown.x,
			top: transform.y * shown.zoom + shown.y,
			width: width * shown.zoom,
			height: height * shown.zoom
		};
	});
	return {
		left: Math.min(...boxes.map((box) => box.left)),
		top: Math.min(...boxes.map((box) => box.top)),
		right: Math.max(...boxes.map((box) => box.left + box.width)),
		bottom: Math.max(...boxes.map((box) => box.top + box.height))
	};
}

describe('fitting after a switch', () => {
	it('fits the whole drawing after an explicit switch, then allows panning', async () => {
		await openGraph({ trees: { combined: fanTree } });
		expect(viewport().fitted).toBe(true);

		// Leave the fitted state, so the switch's fit is observable.
		await dragBackground([100, 100], [180, 160]);
		expect(viewport().fitted).toBe(false);

		await switchTo('Left to right');
		await waitFor(() => expect(viewport().fitted).toBe(true));
		// The fit is saved for the orientation it belongs to, so a reload has a
		// usable record rather than the old panned view.
		await waitFor(() => {
			const stored = JSON.parse(
				localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'viewport')) ?? 'null'
			);
			expect(stored?.direction).toBe('LR');
			expect(stored?.zoom).toBeCloseTo(viewport().zoom, 3);
		});

		// Panning afterwards is the reader's own view: nothing snaps back.
		await dragBackground([100, 100], [130, 120]);
		expect(viewport().fitted).toBe(false);
	});

	it('fits the remaining canvas when the detail panel is open', async () => {
		await openGraph({
			trees: { combined: fanTree },
			items: {
				'wm-fan-root': { ...detail, id: 'wm-fan-root', title: 'Fan root', relationships: {
					parent: null,
					children: ['wm-fan-first', 'wm-fan-second'],
					dependencies: [],
					dependents: [],
					soft_links: []
				} }
			}
		});

		// Open the detail, then give the remaining canvas its reduced size. The
		// browser reports these through the element, exactly as a side panel
		// taking width would.
		await fireEvent.click(screen.getByTestId('graph-select-wm-fan-root'));
		await screen.findByTestId('detail-panel');
		await waitFor(() => expect(drawnBounds().left).toBeGreaterThanOrEqual(0));

		const canvas = screen.getByTestId('graph-canvas');
		Object.defineProperty(canvas, 'clientWidth', { configurable: true, value: 520 });
		Object.defineProperty(canvas, 'clientHeight', { configurable: true, value: 320 });

		await switchTo('Left to right');
		await waitFor(() => expect(viewport().fitted).toBe(true));

		// The fit is measured against the reduced canvas, not the full page.
		const box = drawnBounds();
		expect(box.left).toBeGreaterThanOrEqual(0);
		expect(box.top).toBeGreaterThanOrEqual(0);
		expect(box.right).toBeLessThanOrEqual(520);
		expect(box.bottom).toBeLessThanOrEqual(320);
	});
});

describe('restoring the viewport for the current direction', () => {
	it('restores a viewport saved in the direction being drawn', async () => {
		const namespace = projectIdFor(ROOT_A);
		localStorage.setItem(preferenceKey(namespace, 'direction'), 'LR');
		localStorage.setItem(
			preferenceKey(namespace, 'viewport'),
			JSON.stringify({ x: 40, y: 12, zoom: 1.1, direction: 'LR' })
		);

		await openGraph({ trees: { combined: hierarchyTree } });
		await waitFor(() => expect(viewport().zoom).toBeCloseTo(1.1, 3));
		expect(viewport().fitted).toBe(false);
		expect(checkedDirection()).toBe('LR');
		expect(screen.getByTestId('graph').getAttribute('data-direction')).toBe('LR');
		// And the restored view is of the left-to-right drawing.
		expect(number(card('wm-h-child'), 'data-graph-x')).toBeGreaterThan(
			number(card('wm-h-root'), 'data-graph-x')
		);
	});

	it('fits instead of applying a record for the other direction', async () => {
		const namespace = projectIdFor(ROOT_A);
		localStorage.setItem(preferenceKey(namespace, 'direction'), 'LR');
		localStorage.setItem(
			preferenceKey(namespace, 'viewport'),
			JSON.stringify({ x: 40, y: 12, zoom: 1.1, direction: 'TB' })
		);

		await openGraph({ trees: { combined: hierarchyTree } });
		await waitFor(() => expect(viewport().fitted).toBe(true));
		expect(viewport().zoom).not.toBeCloseTo(1.1, 3);
	});

	it('treats a legacy viewport without a direction as top to bottom', async () => {
		const namespace = projectIdFor(ROOT_A);
		localStorage.setItem(
			preferenceKey(namespace, 'viewport'),
			JSON.stringify({ x: 40, y: 12, zoom: 1.1 })
		);

		const first = await openGraph({ trees: { combined: hierarchyTree } });
		await waitFor(() => expect(viewport().zoom).toBeCloseTo(1.1, 3));
		first.unmount();

		// The same legacy record is not a left-to-right view.
		localStorage.setItem(preferenceKey(namespace, 'direction'), 'LR');
		localStorage.setItem(
			preferenceKey(namespace, 'viewport'),
			JSON.stringify({ x: 40, y: 12, zoom: 1.1 })
		);
		const second = await openGraph({ trees: { combined: hierarchyTree } });
		await waitFor(() => expect(viewport().fitted).toBe(true));
		expect(viewport().zoom).not.toBeCloseTo(1.1, 3);
		second.unmount();
	});
});

describe('remembering the direction', () => {
	it('keeps one direction per project, shared by combined and grouped', async () => {
		const first = await openGraph({ trees: BOTH_VIEWS });
		await switchTo('Left to right');
		expect(localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'direction'))).toBe('LR');

		// The choice belongs to the project, not to one data view: switching to
		// the grouped graph keeps the direction and the layout.
		await fireEvent.click(screen.getByLabelText('By category'));
		await waitFor(() => expect(card('wm-bug-login')).not.toBeNull());
		expect(checkedDirection()).toBe('LR');
		expect(screen.getByTestId('graph').getAttribute('data-direction')).toBe('LR');
		expect(number(card('wm-task-child'), 'data-graph-x')).toBeGreaterThan(
			number(card('wm-bug-login'), 'data-graph-x')
		);
		first.unmount();

		// A refresh reopens the graph it remembered, in the chosen direction.
		const second = await renderLoaded({ trees: BOTH_VIEWS });
		await screen.findByTestId('graph');
		expect(checkedDirection()).toBe('LR');
		expect(screen.getByTestId('graph').getAttribute('data-direction')).toBe('LR');
		second.unmount();
	});

	it('does not leak a direction into another project', async () => {
		const first = await openGraph({ root: ROOT_A, trees: { combined: fanTree } });
		await switchTo('Left to right');
		first.unmount();

		const second = makeFetch({ root: ROOT_B, trees: { combined: fanTree } });
		render(Explorer, { fetchImpl: second.fetchImpl, storage: localStorage });
		await screen.findByTestId('tree-summary');
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		await waitFor(() => expect(cards().length).toBeGreaterThan(0));

		expect(checkedDirection()).toBe('TB');
		expect(screen.getByTestId('graph').getAttribute('data-direction')).toBe('TB');
		expect(number(card('wm-fan-first'), 'data-graph-y')).toBeGreaterThan(
			number(card('wm-fan-root'), 'data-graph-y')
		);
	});
});

describe('what a switch preserves', () => {
	it('keeps the categories, selection, detail, collapse, dependencies, and sibling order', async () => {
		const rendered = await openGroupedGraph({
			trees: BOTH_VIEWS,
			items: { 'wm-bug-login': detail }
		});

		// A real category choice: the quiet Docs category is deselected.
		await clickCategory('docs');
		await fireEvent.keyDown(screen.getByTestId('category-search'), { key: 'Escape' });
		expect(screen.getByTestId('category-trigger').textContent).toContain('Categories (2)');

		// Dependencies on, a branch folded, an item selected with its detail open.
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		await waitFor(() => expect(edge('dependency', 'wm-task-ship', 'wm-bug-login')).not.toBeUndefined());
		await fireEvent.click(screen.getByTestId('graph-toggle-wm-bug-login'));
		await waitFor(() => expect(drawnIds()).not.toContain('wm-task-child'));
		await fireEvent.click(screen.getByTestId('graph-select-wm-bug-login'));
		await screen.findByTestId('detail-panel');
		await waitFor(() =>
			expect(screen.getByTestId('detail-title').textContent).toBe('<b>Login</b> fails')
		);
		const before = drawnIds();
		const beforeRequests = rendered.requests.map((request) => request.url);

		await switchTo('Left to right');

		// Geometry changed; nothing else did.
		expect(screen.getByTestId('graph').getAttribute('data-direction')).toBe('LR');
		expect(drawnIds()).toEqual(before);
		expect(screen.getByTestId('category-trigger').textContent).toContain('Categories (2)');
		expect((screen.getByLabelText('Show dependency edges') as HTMLInputElement).checked).toBe(true);
		expect(edge('dependency', 'wm-task-ship', 'wm-bug-login')).not.toBeUndefined();
		expect(card('wm-bug-login').getAttribute('data-collapsed')).toBe('true');
		expect(screen.getByTestId('graph-toggle-wm-bug-login').getAttribute('aria-expanded')).toBe(
			'false'
		);
		expect(screen.getByTestId('detail-panel')).not.toBeNull();
		expect(screen.getByTestId('detail-title').textContent).toBe('<b>Login</b> fails');
		// Switching reads nothing new and writes nothing to the project.
		expect(rendered.requests.map((request) => request.url)).toEqual(beforeRequests);
		expect(rendered.requests.every((request) => request.method === 'GET')).toBe(true);
	});
});

describe('the direction control and the keyboard', () => {
	it('is a native radio group that announces the selected direction in words', async () => {
		await openGraph({ trees: { combined: fanTree } });

		const group = screen.getByRole('group', { name: 'Graph direction' });
		expect(group.contains(directionControl())).toBe(true);
		const tb = within(group).getByRole('radio', { name: 'Top to bottom' });
		const lr = within(group).getByRole('radio', { name: 'Left to right' });
		expect(tb.tagName).toBe('INPUT');
		expect(lr.tagName).toBe('INPUT');
		expect((tb as HTMLInputElement).checked).toBe(true);
		expect((lr as HTMLInputElement).checked).toBe(false);
		// Words, not an icon or a colour swatch, name both choices.
		expect(within(group).getByText('Top to bottom')).not.toBeNull();
		expect(within(group).getByText('Left to right')).not.toBeNull();

		// A keyboard reader can reach and activate it: it is a focusable native
		// radio, and activation is the browser's own click path.
		lr.focus();
		expect(document.activeElement).toBe(lr);
		expect(lr.tabIndex).toBe(0);
		await fireEvent.click(lr);
		expect((lr as HTMLInputElement).checked).toBe(true);
		expect((tb as HTMLInputElement).checked).toBe(false);
		expect(screen.getByTestId('graph').getAttribute('data-direction')).toBe('LR');
	});
});

describe('when browser storage fails', () => {
	it('keeps switching usable for the session and shows the persistence notice', async () => {
		// Reads work; writes fail, the way a quota or revoked permission does.
		const storage = {
			length: localStorage.length,
			clear: () => localStorage.clear(),
			getItem: (key: string) => localStorage.getItem(key),
			key: (index: number) => localStorage.key(index),
			removeItem: (key: string) => localStorage.removeItem(key),
			setItem: () => {
				throw new Error('quota exceeded');
			}
		} as unknown as Storage;

		await openGraph({ trees: { combined: fanTree } }, storage);
		await switchTo('Left to right');
		expect(screen.getByTestId('graph').getAttribute('data-direction')).toBe('LR');
		expect(number(card('wm-fan-first'), 'data-graph-x')).toBeGreaterThan(
			number(card('wm-fan-root'), 'data-graph-x')
		);
		expect(screen.getByTestId('persistence-notice').textContent).toContain(
			'Preferences could not be saved'
		);

		// The control still works in the other direction for this session.
		await switchTo('Top to bottom');
		expect(screen.getByTestId('graph').getAttribute('data-direction')).toBe('TB');
		expect(number(card('wm-fan-first'), 'data-graph-y')).toBeGreaterThan(
			number(card('wm-fan-root'), 'data-graph-y')
		);
	});
});
