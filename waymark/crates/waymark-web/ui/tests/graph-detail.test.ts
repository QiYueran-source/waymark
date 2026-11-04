import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

// The real stylesheet, so the focus-ring assertion is about what a keyboard
// user actually sees rather than about a class name.
import '../src/app.css';
import Explorer from '$lib/components/Explorer.svelte';
import { preferenceKey, projectIdFor } from '$lib/storage';
import { ROOT_A, combinedTree, detail, makeFetch } from './fixtures';

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

/** The graph's own node cards, in document order. */
function cards(): HTMLElement[] {
	return screen.getAllByTestId(/^graph-node-/);
}

function card(id: string): HTMLElement {
	return screen.getByTestId(`graph-node-${id}`);
}

function nodeButton(id: string): HTMLElement {
	return screen.getByTestId(`graph-select-${id}`);
}

function textOf(element: Element | null): string {
	return element?.textContent?.replace(/\s+/g, ' ').trim() ?? '';
}

/**
 * The item read is held open until `release()` is called, so a test can look at
 * the graph while the detail is still loading.
 */
function gatedItemFetch(options: FetchOptions) {
	const api = makeFetch(options);
	let release!: () => void;
	const gate = new Promise<void>((resolve) => {
		release = resolve;
	});
	const fetchImpl = (async (input: RequestInfo | URL, init?: RequestInit) => {
		if (String(input).startsWith('/api/item/')) await gate;
		return api.fetchImpl(input, init);
	}) as typeof fetch;
	return { ...api, fetchImpl, release: () => release() };
}

/** The fields the detail shows: the same values whichever presentation opened it. */
function detailFields(): Record<string, string> {
	const panel = screen.getByTestId('detail-panel');
	return {
		title: textOf(screen.getByTestId('detail-title')),
		id: textOf(panel.querySelector('.detail-id')),
		status: textOf(screen.getByTestId('detail-status')),
		state: textOf(screen.getByTestId('detail-state')),
		blockedReason: textOf(screen.getByTestId('detail-blocked-reason')),
		owner: textOf(screen.getByTestId('detail-owner')),
		body: textOf(screen.getByTestId('detail-body')),
		customFields: screen
			.getAllByTestId('custom-field-name')
			.map((name) => `${textOf(name)}=${textOf(name.nextElementSibling)}`)
			.join('|')
	};
}

/**
 * The `outline` declarations the shipped stylesheet applies to `element`, split
 * into the ones that paint a ring and the ones that suppress it.
 */
function outlineRulesFor(element: HTMLElement): { on: string[]; off: string[] } {
	const on: string[] = [];
	const off: string[] = [];
	for (const sheet of Array.from(document.styleSheets)) {
		for (const rule of Array.from(sheet.cssRules) as CSSStyleRule[]) {
			const selector = rule.selectorText;
			const outline = rule.style?.outline ?? rule.style?.outlineStyle ?? '';
			if (!selector || !outline || !element.matches(selector)) continue;
			if (outline.startsWith('none') || outline === '0') off.push(`${selector} { ${outline} }`);
			else on.push(`${selector} { ${outline} }`);
		}
	}
	return { on, off };
}

describe('the graph node detail', () => {
	it('opens the same detail the tree opens, with the same fields', async () => {
		const options: FetchOptions = {
			trees: { combined: combinedTree },
			items: { 'wm-bug-login': detail }
		};

		const tree = await renderLoaded(options);
		await fireEvent.click(screen.getByTestId('select-wm-bug-login'));
		await screen.findByTestId('detail-panel');
		await waitFor(() =>
			expect(screen.getByTestId('detail-title').textContent).toBe('<b>Login</b> fails')
		);
		const fromTree = detailFields();
		tree.unmount();

		await openGraph(options);
		await fireEvent.click(nodeButton('wm-bug-login'));

		// There is one detail panel, not a graph-specific second one.
		expect(screen.getAllByTestId('detail-panel')).toHaveLength(1);
		await waitFor(() =>
			expect(screen.getByTestId('detail-title').textContent).toBe('<b>Login</b> fails')
		);
		expect(detailFields()).toEqual(fromTree);
		// The selection is the only read: no tree re-read, no write.
		expect(screen.queryByTestId('node-wm-bug-login')).toBeNull();
		expect(screen.getByTestId('graph')).not.toBeNull();
	});

	it('marks the selected node for as long as its detail is open', async () => {
		const api = gatedItemFetch({
			trees: { combined: combinedTree },
			items: { 'wm-bug-login': detail }
		});
		render(Explorer, { fetchImpl: api.fetchImpl, storage: localStorage });
		await screen.findByTestId('tree-summary');
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		await waitFor(() => expect(cards().length).toBeGreaterThan(0));

		await fireEvent.click(nodeButton('wm-bug-login'));

		// Marked while the read is still in flight …
		expect(card('wm-bug-login').classList.contains('selected')).toBe(true);
		expect(nodeButton('wm-bug-login').getAttribute('aria-current')).toBe('true');
		expect(screen.getByText('Loading item…')).not.toBeNull();

		// … and after the read replaces the panel's content.
		api.release();
		await waitFor(() =>
			expect(screen.getByTestId('detail-title').textContent).toBe('<b>Login</b> fails')
		);
		expect(card('wm-bug-login').classList.contains('selected')).toBe(true);
		expect(nodeButton('wm-bug-login').getAttribute('aria-current')).toBe('true');
		// No other node claims the selection.
		for (const other of cards().filter((element) => element.dataset.nodeId !== 'wm-bug-login')) {
			expect(other.classList.contains('selected')).toBe(false);
		}
		expect(screen.getAllByRole('button', { current: true })).toHaveLength(1);

		await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
		await waitFor(() => expect(screen.queryByTestId('detail-panel')).toBeNull());
		expect(card('wm-bug-login').classList.contains('selected')).toBe(false);
		expect(nodeButton('wm-bug-login').getAttribute('aria-current')).toBeNull();
	});

	it('returns focus to the node button that opened the detail', async () => {
		await openGraph({
			trees: { combined: combinedTree },
			items: { 'wm-bug-login': detail }
		});
		const trigger = nodeButton('wm-bug-login');
		trigger.focus();
		await fireEvent.click(trigger);

		const panel = await screen.findByTestId('detail-panel');
		await waitFor(() => expect(document.activeElement).toBe(panel));

		await fireEvent.keyDown(panel, { key: 'Escape' });
		await waitFor(() => expect(screen.queryByTestId('detail-panel')).toBeNull());
		await waitFor(() => expect(document.activeElement).toBe(trigger));
		// The exact node button, not the pane as a fallback.
		expect(document.activeElement).toBe(screen.getByTestId('graph-select-wm-bug-login'));

		// A pointer-opened detail returns focus the same way.
		await fireEvent.click(trigger);
		const reopened = await screen.findByTestId('detail-panel');
		await waitFor(() => expect(document.activeElement).toBe(reopened));
		await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
		await waitFor(() => expect(document.activeElement).toBe(trigger));
	});

	it('keeps the viewport, the layout, and the presentation across open and close', async () => {
		const rendered = await openGraph({
			trees: { combined: combinedTree },
			items: { 'wm-bug-login': detail }
		});
		const pane = screen.getByLabelText('Work items');
		const viewport = screen.getByTestId('graph').querySelector('.svelte-flow__viewport') as HTMLElement;
		expect(viewport).not.toBeNull();
		const cardBefore = card('wm-bug-login');
		const wrapperBefore = cardBefore.closest('.svelte-flow__node');
		const ys = () =>
			cards().map((element) => `${element.dataset.nodeId}:${element.getAttribute('data-graph-y')}`);
		const ysBefore = ys();

		// Move the viewport away from its starting position, so "unchanged" is
		// observable rather than the default.
		const flowPane = screen.getByTestId('graph').querySelector('.svelte-flow__pane') as HTMLElement;
		await fireEvent.wheel(flowPane, { deltaY: -100, ctrlKey: true });
		const transformBefore = viewport.style.transform;
		expect(transformBefore).not.toBe('translate(0px, 0px) scale(1)');
		const scrollBefore = pane.scrollTop;

		await fireEvent.click(nodeButton('wm-bug-login'));
		await screen.findByTestId('detail-panel');
		await waitFor(() =>
			expect(screen.getByTestId('detail-title').textContent).toBe('<b>Login</b> fails')
		);

		expect(viewport.style.transform).toBe(transformBefore);
		expect(ys()).toEqual(ysBefore);
		expect(card('wm-bug-login')).toBe(cardBefore);
		expect(card('wm-bug-login').closest('.svelte-flow__node')).toBe(wrapperBefore);
		expect(pane.scrollTop).toBe(scrollBefore);
		// The graph is still the presentation, and every node is still drawn.
		expect((screen.getByLabelText('Graph') as HTMLInputElement).checked).toBe(true);
		expect(cards().map((element) => element.dataset.nodeId)).toEqual(
			ysBefore.map((entry) => entry.split(':')[0])
		);

		await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
		await waitFor(() => expect(screen.queryByTestId('detail-panel')).toBeNull());

		expect(viewport.style.transform).toBe(transformBefore);
		expect(ys()).toEqual(ysBefore);
		expect(card('wm-bug-login').closest('.svelte-flow__node')).toBe(wrapperBefore);
		expect(pane.scrollTop).toBe(scrollBefore);
		// Opening and closing a detail stores no graph state of its own.
		expect(localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'expanded'))).toBeNull();
		expect(rendered.requests.every((request) => request.method === 'GET')).toBe(true);
		expect(rendered.requests.map((request) => request.url)).toEqual([
			'/api/health',
			'/api/tree?view=combined',
			'/api/item/wm-bug-login'
		]);
	});

	it('is operable by keyboard alone, with an ordered tab run of real buttons and a visible focus ring', async () => {
		await openGraph({
			trees: { combined: combinedTree },
			items: { 'wm-bug-login': detail }
		});
		const pane = screen.getByLabelText('Work items');
		const expected = [
			// Each node's title, and — for a node with children — its fold control,
			// in drawing order.
			'graph-select-wm-bug-login',
			'graph-toggle-wm-bug-login',
			'graph-select-wm-task-child',
			'graph-select-wm-task-ship',
			// The canvas controls follow the drawing.
			'graph-zoom-out',
			'graph-zoom-in',
			'graph-fit',
			'graph-reset'
		];
		// Every stop in the pane is listed, in order: no invisible tab stops, and
		// no control reachable only by pointer. jsdom can assert this ordered run
		// (DOM order filtered by tabindex), which is the tab order a browser uses;
		// it cannot synthesise a real Tab press, so this test does not pretend to.
		const stops = Array.from(pane.querySelectorAll<HTMLElement>('button, [tabindex]')).filter(
			(element) => element.tabIndex >= 0
		);
		expect(stops.map((element) => element.dataset.testid)).toEqual(expected);
		// jsdom only matches `:focus-visible` after a keyboard event, so this
		// press sets the modality. It proves nothing about tab order — the
		// ordered list above is what does that — but it lets each stop be
		// checked for the shipped focus ring.
		await fireEvent.keyDown(document.body, { key: 'Tab' });
		for (const stop of stops) {
			expect(stop.tagName).toBe('BUTTON');
			expect(stop.getAttribute('type')).toBe('button');
			expect(stop.tabIndex).toBe(0);
			stop.focus();
			expect(document.activeElement).toBe(stop);
			const stopRing = outlineRulesFor(stop);
			expect(stopRing.off).toEqual([]);
			expect(
				stopRing.on.some((declaration) => declaration.includes('solid')),
				`${stop.dataset.testid} has no visible focus rule`
			).toBe(true);
		}

		// A keyboard user who lands on the first node gets a real ring, and the
		// card shows focus within.
		const trigger = nodeButton('wm-bug-login');
		trigger.focus();
		expect(document.activeElement).toBe(trigger);
		expect(trigger.matches(':focus-visible')).toBe(true);
		expect(card('wm-bug-login').matches(':focus-within')).toBe(true);
		const ring = outlineRulesFor(trigger);
		expect(ring.off).toEqual([]);
		expect(ring.on.some((declaration) => declaration.includes('solid'))).toBe(true);

		// Enter opens the detail.
		await fireEvent.keyDown(trigger, { key: 'Enter' });
		let panel = await screen.findByTestId('detail-panel');
		await waitFor(() =>
			expect(screen.getByTestId('detail-title').textContent).toBe('<b>Login</b> fails')
		);
		await waitFor(() => expect(document.activeElement).toBe(panel));

		// Escape closes it and focus returns to the node.
		await fireEvent.keyDown(panel, { key: 'Escape' });
		await waitFor(() => expect(screen.queryByTestId('detail-panel')).toBeNull());
		await waitFor(() => expect(document.activeElement).toBe(trigger));

		// Space does the same, on a different node, and the trigger is the exact
		// element the detail returns focus to.
		const other = nodeButton('wm-task-ship');
		other.focus();
		await fireEvent.keyDown(other, { key: ' ' });
		panel = await screen.findByTestId('detail-panel');
		await waitFor(() => expect(document.activeElement).toBe(panel));
		expect(screen.getByTestId('detail-title').textContent).toBe('wm-task-ship');

		await fireEvent.keyDown(panel, { key: 'Escape' });
		await waitFor(() => expect(screen.queryByTestId('detail-panel')).toBeNull());
		await waitFor(() => expect(document.activeElement).toBe(other));
	});

	it('keeps the detail read-only and its hostile content inert', async () => {
		const rendered = await openGraph({
			trees: { combined: combinedTree },
			items: { 'wm-bug-login': detail }
		});
		const pwned = () => (window as unknown as { waymarkPwned?: boolean }).waymarkPwned;
		(window as unknown as { waymarkPwned?: boolean }).waymarkPwned = undefined;

		await fireEvent.click(nodeButton('wm-bug-login'));
		await screen.findByTestId('detail-panel');
		await waitFor(() =>
			expect(screen.getByTestId('detail-title').textContent).toBe('<b>Login</b> fails')
		);

		const panel = screen.getByTestId('detail-panel');
		// The item's HTML-looking title and custom-field value stay text. Only
		// the description, already rendered and sanitized by the service, is
		// injected as markup — and it is the only such sink.
		expect(screen.getByTestId('detail-title').children).toHaveLength(0);
		expect(screen.getByTestId('detail-title').querySelector('b')).toBeNull();
		expect(panel.textContent).toContain('<script>window.waymarkPwned = true;</script>');
		expect(panel.querySelector('script')).toBeNull();
		expect(pwned()).toBeUndefined();
		expect(screen.getByTestId('detail-body').innerHTML).toBe(detail.body_html);
		// Read-only: no form, no editable region, no write request.
		expect(panel.querySelector('form, input, textarea, [contenteditable]')).toBeNull();
		expect(rendered.requests.every((request) => request.method === 'GET')).toBe(true);
		expect(rendered.requests.map((request) => request.url)).not.toContain(
			'/api/item/wm-bug-login/write'
		);
	});
});
