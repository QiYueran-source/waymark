import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import Explorer from '$lib/components/Explorer.svelte';
import {
	DEPENDENCY_DASH_SCREEN,
	DEPENDENCY_GAP_SCREEN,
	MAX_GRAPH_ZOOM,
	MIN_FIT_ZOOM,
	buildGraph,
	dependencyDashArray,
	dependencyGaps
} from '$lib/graph';
import {
	BUG,
	TASK,
	dependencyTree,
	makeFetch,
	multiDependencyTree,
	treeNode
} from './fixtures';
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

function edges(kind: 'parent-child' | 'dependency'): HTMLElement[] {
	return screen
		.getAllByTestId('graph-edge')
		.filter((edge) => edge.getAttribute('data-edge-kind') === kind);
}

/** The SVG path that carries an edge's stroke, dash, and arrowhead. */
function edgePath(edge: HTMLElement): SVGPathElement {
	const path = edge.querySelector('path');
	expect(path).not.toBeNull();
	return path as SVGPathElement;
}

/** The transform the drawing says it is showing. */
function viewport() {
	const element = screen.getByTestId('graph-viewport');
	const read = (name: string) => Number(element.getAttribute(name));
	return { x: read('data-x'), y: read('data-y'), zoom: read('data-zoom') };
}

/** The two lengths of a drawn dash pattern, in drawing coordinates. */
function drawnPattern(path: SVGPathElement): [number, number] {
	const value = path.style.strokeDasharray;
	expect(value).not.toBe('');
	const parts = value.split(' ').map((part) => Number(part));
	expect(parts).toHaveLength(2);
	return [parts[0], parts[1]];
}

/** A long row of roots, so the default fit is far below the reader's zoom floor. */
function wideDependencyTree(count: number): CombinedTree {
	const roots = Array.from({ length: count }, (_, index) =>
		treeNode({
			id: `wm-wide-${index}`,
			title: `Wide item ${index}`,
			category: index % 2 === 0 ? BUG : TASK
		})
	);
	roots[0] = { ...roots[0], dependencies: [`wm-wide-${count - 1}`] };
	return { view: 'combined', roots, unresolved: [], categories: [BUG, TASK] };
}

/** The two lengths a dash pattern draws with, in drawing units. */
function pattern(zoom: number): [number, number] {
	const parts = dependencyDashArray(zoom)
		.split(' ')
		.map((part) => Number(part));
	expect(parts).toHaveLength(2);
	return [parts[0], parts[1]];
}

describe('the dependency dash pattern', () => {
	it('keeps at least a screen-space dash and gap at every zoom the canvas allows', () => {
		for (const zoom of [MIN_FIT_ZOOM, 0.11, 0.25, 0.5, 1, 1.5, MAX_GRAPH_ZOOM]) {
			const [dash, gap] = pattern(zoom);
			// `stroke-dasharray` is in drawing coordinates, so the drawn path scales
			// the pattern by the zoom. The screen-space result must stay visible.
			expect(dash * zoom).toBeGreaterThanOrEqual(DEPENDENCY_DASH_SCREEN - 0.01);
			expect(gap * zoom).toBeGreaterThanOrEqual(DEPENDENCY_GAP_SCREEN - 0.01);
		}
	});

	it('grows the pattern in drawing units as the reader zooms out', () => {
		const [fitDash] = pattern(0.25);
		const [zoomDash] = pattern(1);
		expect(fitDash).toBeGreaterThan(zoomDash);
		expect(fitDash / zoomDash).toBeCloseTo(4, 1);
	});

	it('keeps a positive gap, so the drawn edge never becomes a solid line', () => {
		for (const zoom of [MIN_FIT_ZOOM, 0.5, 1, MAX_GRAPH_ZOOM]) {
			const [dash, gap] = pattern(zoom);
			expect(dash).toBeGreaterThan(0);
			expect(gap).toBeGreaterThan(0);
		}
	});

	it('falls back to the neutral pattern for a zoom a denominator cannot use', () => {
		expect(dependencyDashArray(0)).toBe(dependencyDashArray(1));
		expect(dependencyDashArray(Number.NaN)).toBe(dependencyDashArray(1));
	});
});

describe('dependency gap groups', () => {
	it('counts the dependency lines that share a generation gap, without merging them', () => {
		const graph = buildGraph(multiDependencyTree, { dependenciesVisible: true });
		const groups = dependencyGaps(graph);
		expect(
			groups.map((group) => [group.key, group.count, group.description, group.edgeIds.length])
		).toEqual([
			['gap:0:1', 3, 'the gap between generations 0 and 1', 3],
			['row:1', 2, 'generation 1 row', 2]
		]);
		// Each dependency keeps its own id: the count describes the gap, it does
		// not replace the lines with one aggregate.
		expect(new Set(groups.flatMap((group) => group.edgeIds)).size).toBe(5);
	});
});

describe('drawn relationship lines', () => {
	it('draws a visible dash at the default fit of a large drawing, not a solid line', async () => {
		await openGraph({ trees: { combined: wideDependencyTree(12) } });
		await fireEvent.click(dependencyToggle());
		await waitFor(() => expect(edges('dependency')).toHaveLength(1));

		const shown = viewport();
		// The wide drawing fits well below the reader's zoom floor: this is the
		// "large project at the default fit" case that used to draw a solid line.
		expect(shown.zoom).toBeLessThan(0.5);
		const path = edgePath(edges('dependency')[0]);
		const [dash, gap] = drawnPattern(path);
		expect(dash * shown.zoom).toBeGreaterThanOrEqual(DEPENDENCY_DASH_SCREEN - 0.01);
		expect(gap * shown.zoom).toBeGreaterThanOrEqual(DEPENDENCY_GAP_SCREEN - 0.01);
	});

	it('recomputes the dash after zooming out, so its screen-space size is kept', async () => {
		await openGraph({ trees: { combined: dependencyTree } });
		await fireEvent.click(dependencyToggle());
		await waitFor(() => expect(edges('dependency')).toHaveLength(2));
		const beforeZoom = viewport().zoom;
		const [beforeDash] = drawnPattern(edgePath(edges('dependency')[0]));

		for (let press = 0; press < 6; press += 1) {
			await fireEvent.click(screen.getByTestId('graph-zoom-out'));
		}
		await waitFor(() => expect(viewport().zoom).toBeLessThan(beforeZoom));

		const shown = viewport();
		const [dash, gap] = drawnPattern(edgePath(edges('dependency')[0]));
		expect(dash).toBeGreaterThan(beforeDash);
		expect(dash * shown.zoom).toBeGreaterThanOrEqual(DEPENDENCY_DASH_SCREEN - 0.01);
		expect(gap * shown.zoom).toBeGreaterThanOrEqual(DEPENDENCY_GAP_SCREEN - 0.01);
	});

	it('gives parent-child and dependency lines different colour and weight, not dash alone', async () => {
		await openGraph({ trees: { combined: dependencyTree } });
		await fireEvent.click(dependencyToggle());
		await waitFor(() => expect(edges('dependency')).toHaveLength(2));

		const solidEdge = edges('parent-child')[0];
		const dashedEdge = edges('dependency')[0];
		const solid = edgePath(solidEdge);
		const dashed = edgePath(dashedEdge);

		// The two kinds are painted differently in both colour and weight, and the
		// classes name the kind for a stylesheet and for tests.
		expect(dashed.style.stroke).not.toBe('');
		expect(solid.style.stroke).not.toBe('');
		expect(dashed.style.stroke).not.toBe(solid.style.stroke);
		expect(Number(dashed.style.strokeWidth)).toBeGreaterThan(Number(solid.style.strokeWidth));
		expect(dashed.style.strokeDasharray).not.toBe('');
		expect(solid.style.strokeDasharray).toBe('');
		expect(dashedEdge.getAttribute('class')).toContain('graph-edge-dependency');
		expect(solidEdge.getAttribute('class')).toContain('graph-edge-parent-child');
	});

	it('offsets a dependency that coincides with a parent-child edge and marks it', async () => {
		await openGraph({ trees: { combined: dependencyTree } });
		await fireEvent.click(dependencyToggle());
		await waitFor(() => expect(edges('dependency')).toHaveLength(2));

		const pair = (
			kind: 'parent-child' | 'dependency',
			from: string,
			to: string
		): HTMLElement => {
			const found = edges(kind).find(
				(edge) => edge.getAttribute('data-from') === from && edge.getAttribute('data-to') === to
			);
			expect(found).not.toBeUndefined();
			return found as HTMLElement;
		};

		// `wm-dep-fix` is both a child of `wm-dep-bug` and depends on it: the two
		// relations cover the same pair of cards, in opposite directions.
		const solid = pair('parent-child', 'wm-dep-bug', 'wm-dep-fix');
		const dashed = pair('dependency', 'wm-dep-fix', 'wm-dep-bug');
		expect(dashed.getAttribute('data-coincident')).toBe('true');

		// Both lines remain drawn, with their own treatment.
		expect(edgePath(solid).style.strokeDasharray).toBe('');
		expect(edgePath(dashed).style.strokeDasharray).not.toBe('');

		// The dependency leaves and arrives on dedicated offset handles, so it no
		// longer sits exactly under the hierarchy's stroke.
		expect(solid.getAttribute('data-source-handle')).toBe('out-bottom');
		expect(solid.getAttribute('data-target-handle')).toBe('in-top');
		expect(dashed.getAttribute('data-source-handle')).toContain('-offset');
		expect(dashed.getAttribute('data-target-handle')).toContain('-offset');
		expect(edgePath(dashed).getAttribute('d')).not.toBe(edgePath(solid).getAttribute('d'));
	});

	it('reports how many dependencies share a gap and keeps each as its own line', async () => {
		await openGraph({ trees: { combined: multiDependencyTree } });
		await fireEvent.click(dependencyToggle());
		await waitFor(() => expect(edges('dependency')).toHaveLength(5));

		const inGap = (key: string): HTMLElement[] =>
			edges('dependency').filter((edge) => edge.getAttribute('data-gap') === key);

		// Three dependencies cross the generation gap and two run along one row;
		// each is still a separate edge with its own stable id.
		const crossing = inGap('gap:0:1');
		const row = inGap('row:1');
		expect(crossing).toHaveLength(3);
		expect(row).toHaveLength(2);
		for (const edge of crossing) expect(edge.getAttribute('data-gap-count')).toBe('3');
		for (const edge of row) expect(edge.getAttribute('data-gap-count')).toBe('2');
		expect(new Set(edges('dependency').map((edge) => edge.getAttribute('data-id'))).size).toBe(5);

		// A visible marker states the count without any pixel geometry, and the
		// hover copy says how to inspect each line.
		const markers = screen.getAllByTestId('graph-gap-count');
		const gapMarker = markers.find((marker) => marker.getAttribute('data-gap') === 'gap:0:1');
		expect(gapMarker).not.toBeUndefined();
		expect(gapMarker?.getAttribute('data-count')).toBe('3');
		expect(gapMarker?.textContent).toContain('3 dependency lines');
		expect(gapMarker?.textContent).toMatch(/hover/i);
	});

	it('keeps both legend entries and adds the hover hint', async () => {
		await openGraph({ trees: { combined: dependencyTree } });
		const legend = screen.getByTestId('graph-legend');
		expect(legend.querySelectorAll('[data-legend-kind]')).toHaveLength(2);
		expect(legend.textContent).toMatch(/Solid line: parent-child/i);
		expect(legend.textContent).toMatch(/Dashed line: dependency/i);

		const hint = screen.getByTestId('graph-legend-hint');
		expect(legend.contains(hint)).toBe(true);
		expect(hint.textContent).toMatch(/hover/i);
		expect(hint.textContent).toMatch(/identif/i);
	});

	it('raises, thickens, and labels an edge on hover while keeping its name', async () => {
		await openGraph({ trees: { combined: dependencyTree } });
		await fireEvent.click(dependencyToggle());
		await waitFor(() => expect(edges('dependency')).toHaveLength(2));

		const from = 'wm-dep-fix';
		const findDependency = (): HTMLElement => {
			const found = edges('dependency').find((edge) => edge.getAttribute('data-from') === from);
			expect(found).not.toBeUndefined();
			return found as HTMLElement;
		};

		const idle = findDependency();
		// The relation is readable without hovering: every edge has its name.
		const name = idle.getAttribute('aria-label');
		expect(name).toBe('Dependency: Fix the login form → Login fails (depends on, across categories)');
		expect(idle.getAttribute('data-hovered')).toBe('false');
		const idleWidth = Number(edgePath(idle).style.strokeWidth);

		await fireEvent.pointerEnter(idle);
		await waitFor(() => expect(findDependency().getAttribute('data-hovered')).toBe('true'));
		const raised = findDependency();
		// Raised above the other edges and thicker, so it is visible at a shared gap.
		expect((raised.closest('svg') as SVGSVGElement).style.zIndex).toBe('1000');
		expect(Number(edgePath(raised).style.strokeWidth)).toBeGreaterThan(idleWidth);
		// Its relation label is shown while hovered.
		const shown = await screen.findByText(name as string);
		expect(shown.textContent).toBe(name);

		await fireEvent.pointerLeave(raised);
		await waitFor(() => expect(findDependency().getAttribute('data-hovered')).toBe('false'));
		expect(screen.queryByText(name as string)).toBeNull();
		expect(Number(edgePath(findDependency()).style.strokeWidth)).toBe(idleWidth);
		expect(findDependency().getAttribute('aria-label')).toBe(name);
	});
});
