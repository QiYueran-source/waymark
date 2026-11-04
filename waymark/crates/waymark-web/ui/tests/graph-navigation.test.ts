import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import Explorer from '$lib/components/Explorer.svelte';
import { preferenceKey, projectIdFor } from '$lib/storage';
import { GRAPH_NOMINAL_CANVAS, MAX_GRAPH_ZOOM, MIN_GRAPH_ZOOM } from '$lib/graph';
import { ROOT_A, ROOT_B, hierarchyTree, makeFetch } from './fixtures';

/**
 * Navigating and folding the drawing.
 *
 * The canvas measures itself in a browser and reports a zero-sized box in a DOM
 * without layout, so the drawing computes its fit from the model's geometry and
 * states it on the readout. These tests read that statement — `data-x`,
 * `data-y`, `data-zoom`, and `data-fitted` — rather than the drawing library's
 * internals, so they describe what a reader sees: whether the whole drawing is
 * in view, where it has been panned to, and how far it is zoomed.
 */

type FetchOptions = Parameters<typeof makeFetch>[0];

async function renderLoaded(options: FetchOptions) {
	const api = makeFetch(options);
	const view = render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
	await screen.findByTestId('tree-summary');
	return { ...api, ...view };
}

async function openGraph(options: FetchOptions) {
	const rendered = await renderLoaded(options);
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

/** The transform the drawing says it is showing. */
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

/**
 * Drag the background, the way a reader pans the canvas.
 *
 * jsdom's `MouseEvent` has no usable `view`, and the drawing's pan handler
 * reads it, so the event is built and then given one. This is the gesture the
 * canvas itself listens for — the same path a mouse or a finger takes.
 */
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
	// A drag ends by suppressing the click that would otherwise follow it, so
	// the next real click is not mistaken for the tail of the gesture. That
	// suppressor is attached to the window and waits for a task of its own
	// before standing down; letting it run keeps the next click honest.
	await new Promise((resolve) => setTimeout(resolve, 0));
}

/** Pinch the canvas, the way a trackpad or touch screen zooms it. */
async function pinch(deltaY: number) {
	await fireEvent.wheel(pane(), { deltaY, ctrlKey: true, clientX: 100, clientY: 100 });
}

/** Scroll-wheeling with the two-finger gesture pans; the drawing reports it. */
async function trackpadPan(deltaX: number, deltaY: number) {
	await fireEvent.wheel(pane(), { deltaX, deltaY, clientX: 100, clientY: 100 });
}

/**
 * A finger, or two, on the canvas.
 *
 * jsdom has a `TouchEvent` but no `Touch` constructor, so the events are built
 * as plain `Event`s and given the touch lists the canvas reads. That is the
 * same path a real touch screen produces: the renderer listens for `touchstart`,
 * `touchmove`, and `touchend` on the pane.
 */
function touchEvents(points: Array<[number, number]>): unknown[] {
	return points.map(([x, y], index) => ({
		identifier: index + 1,
		target: pane(),
		clientX: x,
		clientY: y,
		pageX: x,
		pageY: y,
		screenX: x,
		screenY: y,
		radiusX: 1,
		radiusY: 1,
		force: 1
	}));
}

async function touch(type: 'touchstart' | 'touchmove' | 'touchend', points: Array<[number, number]>) {
	const event = new Event(type, { bubbles: true, cancelable: true });
	const touches = touchEvents(points);
	Object.defineProperty(event, 'touches', { value: touches });
	Object.defineProperty(event, 'targetTouches', { value: touches });
	Object.defineProperty(event, 'changedTouches', { value: touches });
	pane().dispatchEvent(event);
	// The renderer coalesces a gesture's moves; let them flush.
	await new Promise((resolve) => setTimeout(resolve, 50));
}

describe('the drawing on entry', () => {
	it('opens fitted to the whole hierarchy instead of at an unzoomed corner', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		const shown = viewport();
		expect(shown.fitted).toBe(true);
		expect(shown.zoom).toBeGreaterThanOrEqual(MIN_GRAPH_ZOOM);
		expect(shown.zoom).toBeLessThanOrEqual(MAX_GRAPH_ZOOM);

		// Every card, put through the transform, lands inside the canvas: the
		// whole hierarchy is in view, not just its top-left corner.
		for (const element of cards()) {
			const wrapper = element.closest('.svelte-flow__node') as HTMLElement;
			const { transform } = parseTransform(wrapper);
			expect(transform.x * shown.zoom + shown.x).toBeGreaterThanOrEqual(0);
			expect(transform.y * shown.zoom + shown.y).toBeGreaterThanOrEqual(0);
			expect(transform.x * shown.zoom + shown.x).toBeLessThan(GRAPH_NOMINAL_CANVAS.width);
			expect(transform.y * shown.zoom + shown.y).toBeLessThan(GRAPH_NOMINAL_CANVAS.height);
		}
	});

	it('centres the drawing rather than pinning it to a corner', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		const shown = viewport();
		// The drawing's own bounding box, transformed, has equal space on both
		// sides. A fit that merely scaled the drawing would not.
		const boxes = cards().map((element) => {
			const wrapper = element.closest('.svelte-flow__node') as HTMLElement;
			const { width, height, transform } = parseTransform(wrapper);
			return {
				left: transform.x * shown.zoom + shown.x,
				top: transform.y * shown.zoom + shown.y,
				width: width * shown.zoom,
				height: height * shown.zoom
			};
		});
		const left = Math.min(...boxes.map((box) => box.left));
		const right = Math.max(...boxes.map((box) => box.left + box.width));
		const top = Math.min(...boxes.map((box) => box.top));
		const bottom = Math.max(...boxes.map((box) => box.top + box.height));
		const leftGap = left;
		const rightGap = GRAPH_NOMINAL_CANVAS.width - right;
		const topGap = top;
		const bottomGap = GRAPH_NOMINAL_CANVAS.height - bottom;
		// Within a pixel: the fit rounds its margins to whole pixels.
		expect(Math.abs(leftGap - rightGap)).toBeLessThanOrEqual(1);
		expect(Math.abs(topGap - bottomGap)).toBeLessThanOrEqual(1);
	});

	it('fits to a drawing that a fold has shortened', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		const before = viewport();
		const beforeHeight = bounds().bottom - bounds().top;

		await fireEvent.click(screen.getByTestId('graph-toggle-wm-h-child'));
		await waitFor(() => expect(drawnIds()).not.toContain('wm-h-grandchild'));
		// Folding takes a row away; fitting then has less to show, so it can
		// afford a larger zoom than the full drawing did.
		expect(bounds().bottom - bounds().top).toBeLessThan(beforeHeight);

		await fireEvent.click(screen.getByTestId('graph-fit'));
		await waitFor(() => expect(viewport().fitted).toBe(true));
		expect(viewport().zoom).toBeGreaterThan(before.zoom);
	});
});

describe('panning and zooming', () => {
	it('pans with the background drag and reports where the canvas went', async () => {
		const { requests } = await openGraph({ trees: { combined: hierarchyTree } });
		const before = viewport();
		await dragBackground([100, 100], [160, 140]);
		const after = viewport();
		expect(after.x).not.toBe(before.x);
		expect(after.y).not.toBe(before.y);
		expect(after.zoom).toBe(before.zoom);
		expect(after.fitted).toBe(false);
		// Navigating costs no read: the drawing moved, the data did not.
		expect(requests.every((request) => request.method === 'GET')).toBe(true);
	});

	it('pans with the trackpad gesture and zooms with the pinch gesture', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		const start = viewport();

		// Two fingers on a trackpad scroll; the canvas pans instead of zooming.
		await trackpadPan(40, 120);
		await waitFor(() => expect(viewport().x).not.toBe(start.x));
		const panned = viewport();
		expect(panned.zoom).toBe(start.zoom);

		// A pinch arrives as ctrl+wheel and zooms about the pointer.
		await pinch(-120);
		await waitFor(() => expect(viewport().zoom).toBeGreaterThan(panned.zoom));
	});

	it('pans with one finger and zooms with two, the way a touch screen does', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		const start = viewport();

		// One finger drags the background: the canvas pans, the scale holds.
		await touch('touchstart', [[100, 100]]);
		await touch('touchmove', [[150, 170]]);
		await touch('touchend', []);
		await waitFor(() => expect(viewport().x).not.toBe(start.x));
		const panned = viewport();
		expect(panned.y).not.toBe(start.y);
		expect(panned.zoom).toBe(start.zoom);

		// Two fingers moving apart: the canvas zooms, bounded by its range.
		await touch('touchstart', [
			[100, 100],
			[200, 200]
		]);
		await touch('touchmove', [
			[80, 80],
			[240, 240]
		]);
		await touch('touchend', []);
		await waitFor(() => expect(viewport().zoom).toBeGreaterThan(panned.zoom));
		expect(viewport().zoom).toBeLessThanOrEqual(MAX_GRAPH_ZOOM);
	});

	it('zooms in and out from the controls, within the drawing’s zoom range', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		const start = viewport();

		await fireEvent.click(screen.getByTestId('graph-zoom-in'));
		await waitFor(() => expect(viewport().zoom).toBeGreaterThan(start.zoom));

		await fireEvent.click(screen.getByTestId('graph-zoom-out'));
		await fireEvent.click(screen.getByTestId('graph-zoom-out'));
		await waitFor(() => expect(viewport().zoom).toBeLessThan(start.zoom));

		// The range is bounded: repeated presses stop at the limit rather than
		// shrinking the drawing out of existence.
		for (let press = 0; press < 20; press += 1) {
			await fireEvent.click(screen.getByTestId('graph-zoom-out'));
		}
		await waitFor(() => expect(viewport().zoom).toBe(MIN_GRAPH_ZOOM));
	});

	it('returns to the fitted entry state on reset, and forgets what was stored', async () => {
		const first = await openGraph({ trees: { combined: hierarchyTree } });
		const entry = viewport();

		await dragBackground([100, 100], [180, 160]);
		await pinch(-120);
		await waitFor(() => expect(viewport().zoom).not.toBe(entry.zoom));
		await waitFor(() =>
			expect(localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'viewport'))).not.toBeNull()
		);

		await fireEvent.click(screen.getByTestId('graph-reset'));
		await waitFor(() => expect(viewport()).toEqual(entry));
		expect(viewport().fitted).toBe(true);
		// Reset means "start again", so a reload fits rather than restoring the
		// transform the reader just discarded.
		expect(localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'viewport'))).toBeNull();
		first.unmount();

		const second = await renderLoaded({ trees: { combined: hierarchyTree } });
		await screen.findByTestId('graph');
		await waitFor(() => expect(viewport().fitted).toBe(true));
		expect(viewport()).toEqual(entry);
		second.unmount();
	});

	it('fits to view from anywhere, without forgetting the remembered viewport', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		const entry = viewport();
		await dragBackground([100, 100], [180, 160]);
		await waitFor(() => expect(viewport().fitted).toBe(false));

		await fireEvent.click(screen.getByTestId('graph-fit'));
		await waitFor(() => expect(viewport().fitted).toBe(true));
		expect(viewport()).toEqual(entry);
		// "Fit" moved the canvas, so the view it landed on is what is remembered.
		await waitFor(() => {
			const stored = JSON.parse(
				localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'viewport')) ?? 'null'
			);
			expect(stored?.zoom).toBeCloseTo(entry.zoom, 2);
		});
	});
});

describe('dragging a node', () => {
	it('refuses the drag and leaves the drawing and its relations as they were', async () => {
		const { requests } = await openGraph({ trees: { combined: hierarchyTree } });
		const element = card('wm-h-child');
		const wrapper = element.closest('.svelte-flow__node') as HTMLElement;
		const style = wrapper.getAttribute('style');
		const edges = edgeTriples();

		// A reader who grabs a card and pulls: the canvas is inert, the card is
		// not draggable, and nothing follows the pointer.
		await fireEvent.pointerDown(element, { button: 0, clientX: 20, clientY: 20 });
		await fireEvent.pointerMove(element, { clientX: 220, clientY: 260 });
		await fireEvent.pointerUp(element, { clientX: 220, clientY: 260 });

		expect(wrapper.getAttribute('style')).toBe(style);
		expect(edgeTriples()).toEqual(edges);
		// The whole hierarchy is still drawn where the layout put it.
		expect(drawnIds()).toEqual(['wm-h-root', 'wm-h-child', 'wm-h-grandchild', 'wm-h-second-root']);
		expect(viewport().fitted).toBe(true);
		// Nothing was written down, and nothing was re-read.
		expect(localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'viewport'))).toBeNull();
		expect(requests.map((request) => request.url)).toEqual([
			'/api/health',
			'/api/tree?view=combined'
		]);
	});
});

describe('folding a branch', () => {
	it('hides the descendants of a collapsed node and restores them on expand', async () => {
		const { requests } = await openGraph({ trees: { combined: hierarchyTree } });
		expect(drawnIds()).toEqual(['wm-h-root', 'wm-h-child', 'wm-h-grandchild', 'wm-h-second-root']);

		await fireEvent.click(screen.getByTestId('graph-toggle-wm-h-child'));
		await waitFor(() => expect(drawnIds()).toEqual(['wm-h-root', 'wm-h-child', 'wm-h-second-root']));
		// The folded node stays, and says it is folded.
		expect(card('wm-h-child').getAttribute('data-collapsed')).toBe('true');
		expect(screen.getByTestId('graph-toggle-wm-h-child').getAttribute('aria-expanded')).toBe(
			'false'
		);
		// Its parent relation is untouched: it is still the child it was.
		expect(card('wm-h-child').getAttribute('data-parent-id')).toBe('wm-h-root');
		expect(card('wm-h-root').getAttribute('data-collapsed')).toBe('false');

		await fireEvent.click(screen.getByTestId('graph-toggle-wm-h-child'));
		await waitFor(() => expect(drawnIds()).toContain('wm-h-grandchild'));
		expect(card('wm-h-child').getAttribute('data-collapsed')).toBe('false');

		// Folding is presentation: nothing was read, written, or re-parented.
		expect(requests.every((request) => request.method === 'GET')).toBe(true);
		expect(requests.map((request) => request.url)).toEqual([
			'/api/health',
			'/api/tree?view=combined'
		]);
	});

	it('hides every generation below a collapsed node, not only the next one', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		await fireEvent.click(screen.getByTestId('graph-toggle-wm-h-root'));
		await waitFor(() =>
			expect(drawnIds()).toEqual(['wm-h-root', 'wm-h-second-root'])
		);
		// The other root and its subtree are untouched.
		expect(screen.getByTestId('graph-toggle-wm-h-root').getAttribute('aria-expanded')).toBe(
			'false'
		);
	});

	it('offers the fold control only on a node that has children', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		expect(screen.queryByTestId('graph-toggle-wm-h-grandchild')).toBeNull();
		expect(screen.getByTestId('graph-toggle-wm-h-root')).not.toBeNull();
	});

	it('folds the structured text alternative the same way', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		await fireEvent.click(screen.getByTestId('graph-toggle-wm-h-child'));
		await waitFor(() => expect(drawnIds()).not.toContain('wm-h-grandchild'));

		await fireEvent.click(screen.getByLabelText('Structured list'));
		await screen.findByTestId('graph-text');
		// One model, two readings: the text list drops exactly what the drawing
		// dropped, and the parent no longer claims a child it is not showing.
		const listed = screen
			.getAllByTestId(/^graph-text-item-/)
			.map((element) => element.getAttribute('data-node-id'));
		expect(listed).toEqual(['wm-h-root', 'wm-h-child', 'wm-h-second-root']);
		expect(screen.queryByTestId('graph-text-item-wm-h-grandchild')).toBeNull();
		// The folded node is still described, and no longer claims the child it
		// is not drawing: the two readings cannot disagree about the branch.
		expect(screen.getByTestId('graph-text-item-wm-h-child')).not.toBeNull();
		expect(screen.queryByTestId('graph-text-children-wm-h-child')).toBeNull();
	});
});

describe('what the graph remembers', () => {
	it('restores the remembered collapse set and viewport for the same project', async () => {
		const first = await openGraph({ trees: { combined: hierarchyTree } });
		await fireEvent.click(screen.getByTestId('graph-toggle-wm-h-child'));
		await waitFor(() => expect(drawnIds()).not.toContain('wm-h-grandchild'));
		await dragBackground([100, 100], [160, 140]);
		const moved = viewport();
		await waitFor(() =>
			expect(localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'viewport'))).not.toBeNull()
		);
		first.unmount();

		const second = await renderLoaded({ trees: { combined: hierarchyTree } });
		await screen.findByTestId('graph');
		await waitFor(() => expect(cards().length).toBeGreaterThan(0));
		// The branch is still folded, and the canvas is where it was left.
		expect(drawnIds()).not.toContain('wm-h-grandchild');
		expect(viewport().x).toBeCloseTo(moved.x, 1);
		expect(viewport().y).toBeCloseTo(moved.y, 1);
		expect(viewport().zoom).toBeCloseTo(moved.zoom, 3);
		second.unmount();
	});

	it('keeps the collapse set and viewport out of another project', async () => {
		const first = await openGraph({ root: ROOT_A, trees: { combined: hierarchyTree } });
		await fireEvent.click(screen.getByTestId('graph-toggle-wm-h-child'));
		await waitFor(() => expect(drawnIds()).not.toContain('wm-h-grandchild'));
		await dragBackground([100, 100], [160, 140]);
		const moved = viewport();
		first.unmount();

		const second = makeFetch({ root: ROOT_B, trees: { combined: hierarchyTree } });
		render(Explorer, { fetchImpl: second.fetchImpl, storage: localStorage });
		await screen.findByTestId('tree-summary');
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		await waitFor(() => expect(cards().length).toBeGreaterThan(0));
		// B has its own state: nothing is folded, and the canvas is fitted.
		expect(drawnIds()).toContain('wm-h-grandchild');
		expect(viewport().fitted).toBe(true);
		expect(viewport().x).not.toBeCloseTo(moved.x, 0);
	});

	it('stores graph state apart from the tree’s expansion and scroll', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		await fireEvent.click(screen.getByTestId('graph-toggle-wm-h-child'));
		await dragBackground([100, 100], [160, 140]);
		await waitFor(() =>
			expect(localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'viewport'))).not.toBeNull()
		);

		const namespace = projectIdFor(ROOT_A);
		const keys = Object.keys(localStorage);
		expect(keys).toContain(preferenceKey(namespace, 'collapsed'));
		expect(keys).toContain(preferenceKey(namespace, 'viewport'));
		// Nothing about the drawing was written over the tree's own keys.
		expect(localStorage.getItem(preferenceKey(namespace, 'expanded', 'combined'))).toBeNull();
		expect(localStorage.getItem(preferenceKey(namespace, 'scroll', 'combined'))).toBeNull();
		expect(JSON.parse(localStorage.getItem(preferenceKey(namespace, 'collapsed')) ?? '[]')).toEqual([
			'wm-h-child'
		]);
	});

	it('ignores stored node ids the current data does not contain', async () => {
		const namespace = projectIdFor(ROOT_A);
		localStorage.setItem(
			preferenceKey(namespace, 'collapsed'),
			JSON.stringify(['wm-gone', 'wm-h-child'])
		);
		localStorage.setItem(
			preferenceKey(namespace, 'viewport'),
			JSON.stringify({ x: 40, y: 12, zoom: 1.1 })
		);

		await openGraph({ trees: { combined: hierarchyTree } });
		// The id the data no longer has hides nothing and breaks nothing: the
		// rest of the stored set still applies.
		expect(drawnIds()).toEqual(['wm-h-root', 'wm-h-child', 'wm-h-second-root']);
		expect(screen.getByTestId('graph-toggle-wm-h-child').getAttribute('aria-expanded')).toBe(
			'false'
		);
		expect(viewport().zoom).toBeCloseTo(1.1, 3);
	});

	it('falls back to a fit when the stored viewport is unreadable', async () => {
		localStorage.setItem(preferenceKey(projectIdFor(ROOT_A), 'viewport'), '{"x":"left"}');
		await openGraph({ trees: { combined: hierarchyTree } });
		expect(viewport().fitted).toBe(true);
	});
});

describe('when browser storage is unavailable', () => {
	it('folds and pans for the session, and says once that nothing was saved', async () => {
		const api = makeFetch({ trees: { combined: hierarchyTree } });
		render(Explorer, { fetchImpl: api.fetchImpl, storage: null });
		await screen.findByTestId('tree-summary');
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		await waitFor(() => expect(cards().length).toBeGreaterThan(0));

		await fireEvent.click(screen.getByTestId('graph-toggle-wm-h-child'));
		await waitFor(() => expect(drawnIds()).not.toContain('wm-h-grandchild'));
		await dragBackground([100, 100], [160, 140]);
		await fireEvent.click(screen.getByTestId('graph-zoom-in'));
		await waitFor(() => expect(viewport().fitted).toBe(false));

		// Both controls still work; only the remembering does not.
		await fireEvent.click(screen.getByTestId('graph-toggle-wm-h-child'));
		await waitFor(() => expect(drawnIds()).toContain('wm-h-grandchild'));

		expect(screen.getAllByTestId('persistence-notice')).toHaveLength(1);
		expect(screen.getByTestId('persistence-notice').textContent).toContain(
			'Preferences could not be saved'
		);
	});
});

/** The drawn relations, as `kind:from->to`, in document order. */
function edgeTriples(): string[] {
	const drawn = screen.getByTestId('graph');
	return Array.from(drawn.querySelectorAll('[data-testid="graph-edge"]')).map(
		(edge) =>
			`${edge.getAttribute('data-edge-kind')}:${edge.getAttribute('data-from')}->${edge.getAttribute('data-to')}`
	);
}

/**
 * A node wrapper's placed position and size, as the renderer wrote them.
 *
 * The renderer positions each node with an inline transform and states its
 * fixed size in the same inline style; both are geometry the model decided, so
 * a test can put them through the viewport and ask where the card really is.
 */
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
function bounds() {
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
