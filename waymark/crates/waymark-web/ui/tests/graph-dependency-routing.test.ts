import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import Explorer from '$lib/components/Explorer.svelte';
import {
	GRAPH_NODE_WIDTH,
	dependencyRoutes,
	type DependencyRoute,
	type PlacedCard
} from '$lib/graph';
import { BUG, TASK, makeFetch, treeNode } from './fixtures';
import type { CombinedTree } from '$lib/types';

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
	await waitFor(() => expect(screen.getAllByTestId(/^graph-node-/).length).toBeGreaterThan(0));
	return rendered;
}

function dependencyToggle(): HTMLInputElement {
	return screen.getByLabelText('Show dependency edges') as HTMLInputElement;
}

function dependencyEdge(from: string, to: string): HTMLElement {
	const found = screen
		.getAllByTestId('graph-edge')
		.filter((edge) => edge.getAttribute('data-edge-kind') === 'dependency')
		.find(
			(edge) => edge.getAttribute('data-from') === from && edge.getAttribute('data-to') === to
		);
	expect(found).not.toBeUndefined();
	return found as HTMLElement;
}

/** Every point the drawn path passes through, in drawing coordinates. */
function pathPoints(path: SVGPathElement): Array<{ x: number; y: number }> {
	const d = path.getAttribute('d') ?? '';
	const numbers = (d.match(/-?\d+(?:\.\d+)?/g) ?? []).map((part) => Number(part));
	expect(numbers.length).toBeGreaterThanOrEqual(4);
	const points: Array<{ x: number; y: number }> = [];
	for (let index = 0; index + 1 < numbers.length; index += 2) {
		points.push({ x: numbers[index], y: numbers[index + 1] });
	}
	return points;
}

/** A card's drawn rectangle, read back from the card's own attributes. */
function cardRect(id: string): { x: number; y: number; width: number; height: number } {
	const card = screen.getByTestId(`graph-node-${id}`);
	const read = (name: string) => Number(card.getAttribute(name));
	return {
		x: read('data-graph-x'),
		y: read('data-graph-y'),
		width: GRAPH_NODE_WIDTH,
		height: read('data-graph-height')
	};
}

/** True when a point sits strictly inside a card, with a little slack at its edge. */
function inside(point: { x: number; y: number }, rect: { x: number; y: number; width: number; height: number }): boolean {
	const slack = 1;
	return (
		point.x > rect.x + slack &&
		point.x < rect.x + rect.width - slack &&
		point.y > rect.y + slack &&
		point.y < rect.y + rect.height - slack
	);
}

function row(...ids: Array<[string, number, number, number]>): PlacedCard[] {
	return ids.map(([id, x, y, height]) => ({
		id,
		depth: 0,
		x,
		y,
		width: 216,
		height
	}));
}

function lows(route: DependencyRoute): number {
	const numbers = (route.path.match(/-?\d+(?:\.\d+)?/g) ?? []).map((part) => Number(part));
	let lowest = -Infinity;
	for (let index = 1; index < numbers.length; index += 2) lowest = Math.max(lowest, numbers[index]);
	return lowest;
}

describe('routing a dependency out of its own generation', () => {
	it('takes a same-generation dependency around the cards standing between its ends', () => {
		// Three cards in one row, the outer two depending on each other with the
		// middle one squarely between them.
		const cards = row(['left', 0, 0, 84], ['middle', 244, 0, 84], ['right', 488, 0, 84]);
		const routes = dependencyRoutes(cards, [{ id: 'dep', from: 'left', to: 'right' }], 'TB');
		const route = routes.get('dep');
		expect(route).not.toBeUndefined();
		if (route === undefined) return;

		// It leaves the source's facing side and arrives at the target's.
		const points = (route.path.match(/-?\d+(?:\.\d+)?/g) ?? []).map(Number);
		expect([points[0], points[1]]).toEqual([216, 42]);
		const last = points.slice(-2);
		expect(last).toEqual([488, 42]);

		// No part of the drawn route runs through the card in between, so the
		// line is no longer hidden by it.
		for (let index = 0; index + 1 < points.length; index += 2) {
			expect(
				inside({ x: points[index], y: points[index + 1] }, {
					x: 244,
					y: 0,
					width: 216,
					height: 84
				})
			).toBe(false);
		}

		// It clears the row rather than running along it.
		expect(lows(route)).toBeGreaterThan(84 + 6);
	});

	it('leaves an unobstructed pair of neighbours on its direct route', () => {
		// Nothing stands between the two cards, so there is nothing to route
		// around: a detour would only add length and bend.
		const cards = row(['left', 0, 0, 84], ['right', 244, 0, 84]);
		const routes = dependencyRoutes(cards, [{ id: 'dep', from: 'left', to: 'right' }], 'TB');
		expect(routes.size).toBe(0);
	});

	it('gives two dependencies in one row their own lane, so neither hides the other', () => {
		const cards = row(
			['left', 0, 0, 84],
			['middle', 244, 0, 84],
			['right', 488, 0, 84],
			['far', 732, 0, 84]
		);
		const routes = dependencyRoutes(
			cards,
			[
				{ id: 'dep-a', from: 'left', to: 'far' },
				{ id: 'dep-b', from: 'middle', to: 'far' }
			],
			'TB'
		);
		const first = routes.get('dep-a');
		const second = routes.get('dep-b');
		expect(first).not.toBeUndefined();
		expect(second).not.toBeUndefined();
		if (first === undefined || second === undefined) return;

		// Separate corridors: the two routes never share a stroke.
		expect(first.path).not.toBe(second.path);
		expect(Math.abs(first.labelY - second.labelY)).toBeGreaterThanOrEqual(4);
	});

	it('leaves a dependency between two generations on its direct route', () => {
		const cards: PlacedCard[] = [
			...row(['top', 0, 0, 84]),
			{ id: 'bottom', depth: 1, x: 0, y: 140, width: 216, height: 84 }
		];
		const routes = dependencyRoutes(cards, [{ id: 'dep', from: 'bottom', to: 'top' }], 'TB');
		expect(routes.size).toBe(0);
	});

	it('routes along a column instead when the reader chose left to right', () => {
		const cards: PlacedCard[] = [
			{ id: 'top', depth: 0, x: 0, y: 0, width: 216, height: 84 },
			{ id: 'middle', depth: 0, x: 0, y: 140, width: 216, height: 84 },
			{ id: 'bottom', depth: 0, x: 0, y: 280, width: 216, height: 84 },
			{ id: 'next', depth: 1, x: 244, y: 0, width: 216, height: 84 }
		];
		const routes = dependencyRoutes(cards, [{ id: 'dep', from: 'top', to: 'bottom' }], 'LR');
		const route = routes.get('dep');
		expect(route).not.toBeUndefined();
		if (route === undefined) return;

		const points = (route.path.match(/-?\d+(?:\.\d+)?/g) ?? []).map(Number);
		const xs: number[] = [];
		for (let index = 0; index + 1 < points.length; index += 2) {
			// The route never cuts back across the column: its ends sit on the
			// cards' right edge and the corridor runs outside them.
			expect(points[index]).toBeGreaterThanOrEqual(216);
			expect(
				inside({ x: points[index], y: points[index + 1] }, {
					x: 0,
					y: 140,
					width: 216,
					height: 84
				})
			).toBe(false);
			xs.push(points[index]);
		}
		// It detours beside the column instead of running down through it.
		expect(Math.max(...xs)).toBeGreaterThan(216 + 6);
	});
});

/**
 * Three roots in one row, the first depending on the last, with the middle one
 * standing between them: the shape that used to draw a dashed line straight
 * through a card.
 */
function bridgingTree(): CombinedTree {
	const roots = [
		treeNode({ id: 'wm-bridge-left', title: 'Bridge left', category: BUG }),
		treeNode({ id: 'wm-bridge-middle', title: 'Bridge middle', category: TASK }),
		treeNode({ id: 'wm-bridge-right', title: 'Bridge right', category: BUG })
	];
	roots[0] = { ...roots[0], dependencies: ['wm-bridge-right'] };
	return { view: 'combined', roots, unresolved: [], categories: [BUG, TASK] };
}

describe('drawn dependency routes', () => {
	it('draws a same-generation dependency around the card between its ends', async () => {
		await openGraph({ trees: { combined: bridgingTree() } });
		await fireEvent.click(dependencyToggle());
		await waitFor(() => expect(dependencyEdge('wm-bridge-left', 'wm-bridge-right')).not.toBeNull());

		const edge = dependencyEdge('wm-bridge-left', 'wm-bridge-right');
		const path = edge.querySelector('path') as SVGPathElement;
		expect(path).not.toBeNull();

		const middle = cardRect('wm-bridge-middle');
		const points = pathPoints(path);
		for (const point of points) expect(inside(point, middle)).toBe(false);

		// The route clears the whole row, so it is drawn below every card in it.
		const rowBottom = Math.max(
			cardRect('wm-bridge-left').y + cardRect('wm-bridge-left').height,
			middle.y + middle.height,
			cardRect('wm-bridge-right').y + cardRect('wm-bridge-right').height
		);
		expect(Math.max(...points.map((point) => point.y))).toBeGreaterThan(rowBottom + 6);
	});

	it('keeps a routed relation hoverable, raised, and labelled on its route', async () => {
		await openGraph({ trees: { combined: bridgingTree() } });
		await fireEvent.click(dependencyToggle());
		await waitFor(() => expect(dependencyEdge('wm-bridge-left', 'wm-bridge-right')).not.toBeNull());

		const idle = dependencyEdge('wm-bridge-left', 'wm-bridge-right');
		const name = idle.getAttribute('aria-label') as string;
		expect(name).toBe('Dependency: Bridge left → Bridge right (depends on)');
		// The relation is readable without hovering, exactly as on a straight edge.
		expect(idle.getAttribute('data-hovered')).toBe('false');
		const idleWidth = Number((idle.querySelector('path') as SVGPathElement).style.strokeWidth);

		await fireEvent.pointerEnter(idle);
		await waitFor(() =>
			expect(dependencyEdge('wm-bridge-left', 'wm-bridge-right').getAttribute('data-hovered')).toBe(
				'true'
			)
		);
		const raised = dependencyEdge('wm-bridge-left', 'wm-bridge-right');
		expect((raised.closest('svg') as SVGSVGElement).style.zIndex).toBe('1000');
		expect(Number((raised.querySelector('path') as SVGPathElement).style.strokeWidth)).toBeGreaterThan(
			idleWidth
		);

		// The label sits on the route itself — along the lane it clears the row
		// with — not at the midpoint of the straight line it replaced.
		const path = raised.querySelector('path') as SVGPathElement;
		const lane = Math.max(...pathPoints(path).map((point) => point.y));
		const label = await screen.findByText(name);
		const offset = /translate\((-?[\d.]+)px,\s*(-?[\d.]+)px\)/.exec(
			label.getAttribute('style') ?? ''
		);
		expect(offset).not.toBeNull();
		expect(Number(offset?.[2])).toBe(lane);

		await fireEvent.pointerLeave(raised);
		await waitFor(() =>
			expect(dependencyEdge('wm-bridge-left', 'wm-bridge-right').getAttribute('data-hovered')).toBe(
				'false'
			)
		);
		expect(screen.queryByText(name)).toBeNull();
	});
});
