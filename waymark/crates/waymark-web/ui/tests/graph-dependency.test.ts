import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import Explorer from '$lib/components/Explorer.svelte';
import { preferenceKey, projectIdFor } from '$lib/storage';
import {
	ROOT_A,
	combinedTree,
	dependencyTree,
	hierarchyTree,
	makeFetch
} from './fixtures';

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

/** The dependency toggle, wherever the current presentation puts it. */
function dependencyToggle(): HTMLInputElement {
	return screen.getByLabelText('Show dependency edges') as HTMLInputElement;
}

function edges(kind: 'parent-child' | 'dependency'): HTMLElement[] {
	return screen
		.getAllByTestId('graph-edge')
		.filter((edge) => edge.getAttribute('data-edge-kind') === kind);
}

/** The SVG path that carries an edge's stroke and arrowhead. */
function edgePath(edge: HTMLElement): SVGPathElement {
	const path = edge.querySelector('path');
	expect(path).not.toBeNull();
	return path as SVGPathElement;
}

describe('dependency edges in the graph', () => {
	it('draws no dependency edge while dependency visibility is off', async () => {
		await openGraph({ trees: { combined: dependencyTree } });
		expect(dependencyToggle().checked).toBe(false);
		expect(edges('dependency')).toHaveLength(0);
		// Only the hierarchy is drawn, and it is still solid.
		expect(edges('parent-child')).toHaveLength(1);
	});

	it('draws one dashed edge per dependency of a drawn item when enabled', async () => {
		await openGraph({ trees: { combined: dependencyTree } });
		await fireEvent.click(dependencyToggle());

		await waitFor(() => expect(edges('dependency')).toHaveLength(2));
		expect(
			edges('dependency').map((edge) => [
				edge.getAttribute('data-from'),
				edge.getAttribute('data-to')
			])
		).toEqual([
			['wm-dep-fix', 'wm-dep-bug'],
			['wm-dep-ship', 'wm-dep-bug']
		]);

		// A dependency on an item this drawing does not contain yields no edge.
		expect(
			screen
				.getAllByTestId('graph-edge')
				.some((edge) => edge.getAttribute('data-to') === 'wm-dep-archived')
		).toBe(false);
	});

	it('draws dependency edges dashed, with an arrowhead pointing at the depended-on item', async () => {
		await openGraph({ trees: { combined: dependencyTree } });
		await fireEvent.click(dependencyToggle());
		await waitFor(() => expect(edges('dependency')).toHaveLength(2));

		const labelByFrom: Record<string, string> = {
			'wm-dep-fix': 'Dependency: Fix the login form → Login fails (depends on, across categories)',
			'wm-dep-ship': 'Dependency: Ship the release → Login fails (depends on, across categories)'
		};
		for (const edge of edges('dependency')) {
			const path = edgePath(edge);
			// Dashed: a dash pattern on the drawn path.
			expect(path.getAttribute('style') ?? '').toContain('dasharray');
			// Directed: the arrow is at the target end, and the app marks which end
			// it points at so the direction is assertable without pixel geometry.
			expect(path.getAttribute('marker-end')).toMatch(/^url\(/);
			expect(path.getAttribute('marker-start')).toBeNull();
			expect(edge.getAttribute('data-arrow')).toBe('target');
			// Readable without hovering: the accessible name states the direction.
			expect(edge.getAttribute('aria-label')).toBe(
				labelByFrom[edge.getAttribute('data-from') ?? '']
			);
		}

		// The hierarchy edge is still solid and undirected.
		for (const edge of edges('parent-child')) {
			const path = edgePath(edge);
			expect(path.getAttribute('style') ?? '').not.toContain('dasharray');
			expect(path.getAttribute('marker-end')).toBeNull();
		}
	});

	it('keeps the parent-child edge when the same pair is also a dependency', async () => {
		await openGraph({ trees: { combined: dependencyTree } });
		await fireEvent.click(dependencyToggle());
		await waitFor(() => expect(edges('dependency')).toHaveLength(2));

		const pair = (edge: HTMLElement): string =>
			`${edge.getAttribute('data-from')}->${edge.getAttribute('data-to')}`;
		// `wm-dep-fix` sits under `wm-dep-bug` and depends on it: both relations
		// are drawn, in their own directions, and neither hides the other.
		const solid = edges('parent-child').map(pair);
		const dashed = edges('dependency').map(pair);
		expect(solid).toContain('wm-dep-bug->wm-dep-fix');
		expect(dashed).toContain('wm-dep-fix->wm-dep-bug');
	});

	it('draws a cross-category dependency in the combined graph', async () => {
		await openGraph({ trees: { combined: dependencyTree } });
		await fireEvent.click(dependencyToggle());
		await waitFor(() => expect(edges('dependency')).toHaveLength(2));

		// `wm-dep-ship` (Tasks) depends on `wm-dep-bug` (Bugs).
		const cross = edges('dependency').find(
			(edge) => edge.getAttribute('data-from') === 'wm-dep-ship'
		);
		expect(cross?.getAttribute('data-to')).toBe('wm-dep-bug');
		expect(cross?.getAttribute('aria-label')).toContain('Ship the release');
	});

	it('defines both line treatments in the one legend panel', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		const legend = screen.getByTestId('graph-legend');
		const solid = legend.querySelector('[data-legend-kind="parent-child"]');
		const dashed = legend.querySelector('[data-legend-kind="dependency"]');
		expect(solid).not.toBeNull();
		expect(dashed).not.toBeNull();
		expect(legend.textContent).toMatch(/Solid line: parent-child/i);
		expect(legend.textContent).toMatch(/Dashed line: dependency/i);
		expect(legend.textContent).toMatch(/arrow points at the depended-on item/i);
		// The dashed swatch is drawn dashed, not merely labelled.
		expect(dashed?.querySelector('line')?.getAttribute('stroke-dasharray')).not.toBeNull();
	});

	it('keeps the remembered dependency state across a presentation switch', async () => {
		await openGraph({ trees: { combined: dependencyTree } });
		await fireEvent.click(dependencyToggle());
		await waitFor(() => expect(edges('dependency')).toHaveLength(2));

		await fireEvent.click(screen.getByLabelText('Tree'));
		await screen.findByTestId('node-wm-dep-bug');
		expect(dependencyToggle().checked).toBe(true);
		// The tree's own overlay is on too: the one toggle governs both
		// presentations, so its remembered state is shared, not per presentation.
		expect(screen.getAllByTestId('dependency-edge').length).toBeGreaterThan(0);

		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		expect(dependencyToggle().checked).toBe(true);
		await waitFor(() => expect(edges('dependency')).toHaveLength(2));
	});

	it('keeps the remembered dependency state across a reload, per view', async () => {
		const first = await openGraph({ trees: { combined: dependencyTree } });
		await fireEvent.click(dependencyToggle());
		await waitFor(() => expect(edges('dependency')).toHaveLength(2));
		expect(
			localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'dependencies', 'combined'))
		).toBe('true');
		first.unmount();

		const second = await renderLoaded({ trees: { combined: dependencyTree } });
		await screen.findByTestId('graph');
		expect(dependencyToggle().checked).toBe(true);
		await waitFor(() => expect(edges('dependency')).toHaveLength(2));
		second.unmount();
	});

	it('turning the overlay off removes every dependency edge and leaves the hierarchy', async () => {
		await openGraph({ trees: { combined: dependencyTree } });
		await fireEvent.click(dependencyToggle());
		await waitFor(() => expect(edges('dependency')).toHaveLength(2));

		await fireEvent.click(dependencyToggle());
		await waitFor(() => expect(edges('dependency')).toHaveLength(0));
		expect(edges('parent-child')).toHaveLength(1);
	});

	it('does not move the hierarchy when the overlay is switched on', async () => {
		await openGraph({ trees: { combined: dependencyTree } });
		const rows = () =>
			Object.fromEntries(
				screen
					.getAllByTestId(/^graph-node-/)
					.map((card) => [
						card.getAttribute('data-node-id'),
						card.getAttribute('data-graph-y')
					])
			);
		const before = rows();
		await fireEvent.click(dependencyToggle());
		await waitFor(() => expect(edges('dependency')).toHaveLength(2));
		// The overlay is drawn over the hierarchy, not laid out with it.
		expect(rows()).toEqual(before);
	});

	it('issues no non-GET request while the overlay is on', async () => {
		const { requests, unmount } = await openGraph({ trees: { combined: dependencyTree } });
		await fireEvent.click(dependencyToggle());
		await waitFor(() => expect(edges('dependency')).toHaveLength(2));

		expect(requests.every((request) => request.method === 'GET')).toBe(true);
		// Toggling the overlay is local: the tree and status reads stay at one each.
		expect(requests.filter((request) => request.url === '/api/tree?view=combined')).toHaveLength(
			1
		);
		expect(requests.filter((request) => request.url === '/api/status')).toHaveLength(1);
		unmount();
	});
});

describe('the plain hierarchy is unaffected by the dependency overlay', () => {
	it('draws no dependency edge for a tree without dependencies', async () => {
		await openGraph({ trees: { combined: combinedTree } });
		await fireEvent.click(dependencyToggle());
		await waitFor(() => expect(edges('dependency')).toHaveLength(1));
		expect(edges('parent-child')).toHaveLength(1);
	});
});
