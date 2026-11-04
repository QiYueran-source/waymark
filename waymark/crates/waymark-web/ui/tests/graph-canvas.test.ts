import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

// The real stylesheet: the canvas contract is a layout rule, so the shipped
// declarations are what this test reads.
import '../src/app.css';
import Explorer from '$lib/components/Explorer.svelte';
import { combinedTree, detail, hierarchyTree, makeFetch, unresolvedTree } from './fixtures';

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
	// The drag suppressor stands down after a task; let it, so the next click
	// is not taken for the tail of this gesture.
	await new Promise((resolve) => setTimeout(resolve, 0));
}

/** Every declaration the shipped stylesheet gives this exact selector. */
function declarationsFor(selector: string): Record<string, string> {
	const declarations: Record<string, string> = {};
	for (const sheet of Array.from(document.styleSheets)) {
		for (const rule of Array.from(sheet.cssRules) as CSSStyleRule[]) {
			if (rule.selectorText !== selector) continue;
			for (const property of Array.from(rule.style)) {
				declarations[property] = rule.style.getPropertyValue(property);
			}
		}
	}
	return declarations;
}

describe('the graph canvas', () => {
	it('fills the content area below the toolbar with no fixed frame', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });

		const graphPane = screen.getByTestId('graph');
		const contentArea = graphPane.closest('main')?.querySelector('section[aria-label="Work items"]');
		expect(contentArea).not.toBeNull();
		expect(graphPane.parentElement).toBe(contentArea);
		// The application chrome is still there; nothing switched to a
		// full-screen drawing that would hide it.
		expect(screen.getByRole('banner')).not.toBeNull();
		expect(screen.queryByRole('button', { name: /full.?screen/i })).toBeNull();

		// The pane is a flex column that takes the remaining height, rather than
		// a fixed-height box with a decorative border.
		const paneStyle = declarationsFor('.graph-pane');
		expect(paneStyle['display']).toBe('flex');
		expect(paneStyle['flex-direction']).toBe('column');
		expect(paneStyle['min-height']).toMatch(/^0(px)?$/);
		expect(paneStyle['height']).toBeUndefined();
		expect(paneStyle['border']).toBeUndefined();
		expect(paneStyle['border-width']).toBeUndefined();
		expect(paneStyle['border-radius']).toBeUndefined();

		const hostStyle = declarationsFor('.tree-pane.graph-pane-host');
		expect(hostStyle['display']).toBe('flex');
		expect(hostStyle['flex-direction']).toBe('column');
	});

	it('refits to the remaining canvas when the detail opens, and not again after', async () => {
		await openGraph({
			trees: { combined: combinedTree },
			items: { 'wm-bug-login': detail }
		});
		const entry = viewport();
		expect(entry.fitted).toBe(true);

		// Leave the fitted entry state, so the fit on open is observable.
		await dragBackground([100, 100], [160, 140]);
		const panned = viewport();
		expect(panned.fitted).toBe(false);
		expect(panned).not.toEqual(entry);

		await fireEvent.click(screen.getByTestId('graph-select-wm-bug-login'));
		const panel = await screen.findByTestId('detail-panel');
		// The detail is still a side panel beside the drawing, not on top of it.
		expect(screen.getByTestId('graph').contains(panel)).toBe(false);

		await waitFor(() => expect(viewport().fitted).toBe(true));
		const opened = viewport();
		expect(opened).not.toEqual(panned);

		// Closing does not fit again: the drawing stays where the open left it.
		await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
		await waitFor(() => expect(screen.queryByTestId('detail-panel')).toBeNull());
		expect(viewport()).toEqual(opened);

		// And panning afterwards stays the reader's own view; nothing snaps it
		// back to the fit.
		await dragBackground([100, 100], [130, 120]);
		expect(viewport().fitted).toBe(false);
	});

	it('preserves the pan and zoom when the window resizes, and fits only when asked', async () => {
		await openGraph({ trees: { combined: hierarchyTree } });
		const entry = viewport();
		await dragBackground([100, 100], [160, 140]);
		const panned = viewport();

		// Shrink the drawing's box the way a browser resize does, then tell the
		// view the window changed.
		const canvas = screen.getByTestId('graph-canvas');
		Object.defineProperty(canvas, 'clientWidth', { configurable: true, value: 520 });
		Object.defineProperty(canvas, 'clientHeight', { configurable: true, value: 320 });
		window.dispatchEvent(new Event('resize'));
		// A resize must never auto-fit, so the reader's view is untouched even
		// though the canvas it is fitted against has changed.
		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(viewport()).toEqual(panned);
		expect(viewport().fitted).toBe(false);

		// An explicit fit uses the new canvas size, not the old one.
		await fireEvent.click(screen.getByTestId('graph-fit'));
		await waitFor(() => expect(viewport().fitted).toBe(true));
		const refit = viewport();
		expect(refit).not.toEqual(entry);
		expect(refit.zoom).toBeLessThan(entry.zoom);
	});

	it('puts unresolved relations in a collapsible bottom strip, not over the nodes', async () => {
		await openGraph({ trees: { combined: unresolvedTree } });

		const graphPane = screen.getByTestId('graph');
		const canvas = screen.getByTestId('graph-canvas');
		const drawer = screen.getByTestId('unresolved');

		// The reasons are there in the default state.
		expect(drawer.textContent).toContain('Orphaned work');
		expect(drawer.textContent).toContain('its parent wm-missing is missing');

		// The drawer is a flow sibling of the canvas inside the graph pane: it
		// takes its own space instead of floating over the drawing.
		expect(canvas.parentElement).toBe(graphPane);
		expect(drawer.parentElement).toBe(graphPane);
		expect(graphPane.lastElementChild).toBe(drawer);
		expect(['absolute', 'fixed']).not.toContain(getComputedStyle(drawer).position);

		// A real button, named with the count, so keyboard and screen-reader
		// users can collapse it and know how many entries there are.
		const toggle = screen.getByTestId('unresolved-toggle');
		expect(toggle.tagName).toBe('BUTTON');
		expect(toggle.getAttribute('type')).toBe('button');
		expect(toggle.tabIndex).toBe(0);
		expect(toggle.textContent).toContain('Unresolved relations (1)');
		expect(toggle.getAttribute('aria-expanded')).toBe('true');

		const contentId = toggle.getAttribute('aria-controls');
		expect(contentId).not.toBeNull();
		const content = document.getElementById(contentId ?? '');
		expect(content).not.toBeNull();
		expect((content as HTMLElement).hidden).toBe(false);

		await fireEvent.click(toggle);
		expect(toggle.getAttribute('aria-expanded')).toBe('false');
		expect((content as HTMLElement).hidden).toBe(true);
		// Collapsing hides the list; it does not forget why the items are there.
		expect(screen.getByTestId('unresolved-item').getAttribute('data-reason')).toBe(
			'parent_missing'
		);

		await fireEvent.click(toggle);
		expect(toggle.getAttribute('aria-expanded')).toBe('true');
		expect((content as HTMLElement).hidden).toBe(false);
	});
});
