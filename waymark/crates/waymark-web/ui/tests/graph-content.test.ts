import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';

// The real stylesheet: the containment contract is about the box a reader sees,
// and jsdom can still affirm the wrap and growth declarations that decide it.
import '../src/app.css';
import Explorer from '$lib/components/Explorer.svelte';
import type { CombinedTree, GroupedTree } from '$lib/types';
import { BUG, TASK, deepGroupedTree, hierarchyTree, makeFetch, treeNode } from './fixtures';
import { clickCategory } from './category-helpers';

type FetchOptions = Parameters<typeof makeFetch>[0];

/** Long enough that the card cannot keep it on one line at the card's width. */
const LONG_TITLE =
	'A title long enough that it must wrap inside the node card instead of spilling past its edge';
const LONG_REASON =
	'waiting on the release checklist gate, the security review, and the migration rehearsal to finish';

const LONG_TREE: CombinedTree = {
	view: 'combined',
	roots: [
		treeNode({
			id: 'wm-long-root',
			title: LONG_TITLE,
			category: BUG,
			status: 'in_progress',
			state: 'blocked',
			blocked: true,
			blocked_reason: LONG_REASON
		}),
		treeNode({ id: 'wm-long-short', title: 'Short note', category: TASK })
	],
	unresolved: [],
	categories: [BUG, TASK]
};

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

/** Render the grouped view, then switch to its graph presentation. */
async function openGroupedGraph(options: FetchOptions) {
	const rendered = await renderLoaded(options);
	await fireEvent.click(screen.getByLabelText('By category'));
	await screen.findByTestId('tree-summary');
	await fireEvent.click(screen.getByLabelText('Graph'));
	await screen.findByTestId('graph');
	await waitFor(() => expect(cards().length).toBeGreaterThan(0));
	return rendered;
}

/**
 * Every rule of the shipped stylesheet that matches `element`, in document
 * order. jsdom does not resolve every shorthand, so the stylesheet itself is
 * the honest source for box treatment such as a hatch or a full-card outline.
 */
function matchingStyles(element: HTMLElement): string[] {
	const matched: string[] = [];
	for (const sheet of Array.from(document.styleSheets)) {
		for (const rule of Array.from(sheet.cssRules) as CSSStyleRule[]) {
			const selector = rule.selectorText;
			if (!selector || !element.matches(selector)) continue;
			matched.push(`${selector} { ${rule.style.cssText} }`);
		}
	}
	return matched;
}

/** The graph's own node cards, in document order. */
function cards(): HTMLElement[] {
	return screen.getAllByTestId(/^graph-node-/);
}

function card(id: string): HTMLElement {
	return screen.getByTestId(`graph-node-${id}`);
}

function textOf(element: Element | null): string {
	return element?.textContent?.replace(/\s+/g, ' ').trim() ?? '';
}

function number(cardElement: HTMLElement, attribute: string): number {
	const raw = cardElement.getAttribute(attribute);
	expect(raw).not.toBeNull();
	return Number(raw);
}

/** The transform the drawing says it is showing. */
function viewport() {
	const element = screen.getByTestId('graph-viewport');
	return {
		x: Number(element.getAttribute('data-x')),
		y: Number(element.getAttribute('data-y')),
		zoom: Number(element.getAttribute('data-zoom')),
		fitted: element.getAttribute('data-fitted') === 'true'
	};
}

describe('node content containment', () => {
	it('wraps a long title and blocked reason inside the card, which grows instead of clipping', async () => {
		await openGraph({ trees: { combined: LONG_TREE } });
		const root = card('wm-long-root');
		const title = root.querySelector('.graph-node-title') as HTMLElement;
		const reason = screen.getByTestId('graph-blocked-reason-wm-long-root');

		// Every piece of node text is a descendant of the card, and none of it is
		// truncated: the whole server value is rendered.
		expect(title.textContent).toBe(LONG_TITLE);
		expect(textOf(reason)).toBe(`! ${LONG_REASON}`);
		expect(title.closest('.graph-node')).toBe(root);
		expect(reason.closest('.graph-node')).toBe(root);

		// Wrapping is the shipped CSS contract, not a component style attribute.
		for (const element of [title, reason]) {
			const style = getComputedStyle(element);
			expect(style.whiteSpace).toBe('normal');
			expect(style.overflowWrap).toBe('anywhere');
		}

		// The card is not a fixed-height frame: its content decides the height, and
		// it keeps a deterministic floor for the short case.
		const cardStyle = getComputedStyle(root);
		expect(cardStyle.height).not.toBe('100%');
		expect(cardStyle.minHeight).not.toBe('auto');
	});
});

describe('context nodes', () => {
	it('keeps the ordinary card and marks context with a full-card outline and an inside badge', async () => {
		await openGroupedGraph({ trees: { combined: LONG_TREE, grouped: deepGroupedTree } });
		// Task-only scope: the Docs and Bug ancestors stay to keep the task under
		// its real parents, and are marked as context rather than selected work.
		await clickCategory('bug');
		await clickCategory('docs');
		await waitFor(() => expect(card('wm-bug-mid').getAttribute('data-context')).toBe('true'));

		const contextCard = card('wm-bug-mid');
		const ordinaryCard = card('wm-task-leaf');
		const marker = screen.getByTestId('graph-context-marker-wm-bug-mid');

		// The badge is content of the card, in the card's flow.
		expect(contextCard.contains(marker)).toBe(true);
		expect(marker.getAttribute('data-context-badge')).toBe('inside');
		expect(textOf(marker)).toBe('Context — outside the selection');
		// No context label floats outside a node.
		expect(marker.closest('.graph-node')).toBe(contextCard);
		const floating = Array.from(document.querySelectorAll('.context-marker')).filter(
			(element) => element.closest('.graph-node') === null
		);
		expect(floating).toHaveLength(0);

		// Ordinary layout: the same category, state, title, and reason structure as
		// any other card, plus the badge; no special card shape.
		expect(contextCard.querySelector('.graph-node-title')).not.toBeNull();
		expect(screen.getByTestId('graph-state-wm-bug-mid')).not.toBeNull();
		expect(screen.getByTestId('graph-blocked-reason-wm-bug-mid')).not.toBeNull();
		expect(contextCard.querySelectorAll('.graph-node-line')).toHaveLength(
			ordinaryCard.querySelectorAll('.graph-node-line').length
		);
		// The visual badge is not the only cue: the node's name states the context.
		expect(screen.getByTestId('graph-select-wm-bug-mid').getAttribute('aria-label')).toBe(
			'Middle bug — Bugs, Blocked (context, outside the selection)'
		);

		// The treatment is a full-card dashed outline, without the old hatch or a
		// left state stripe. The stylesheet is the source of truth for the box.
		expect(contextCard.getAttribute('data-context-treatment')).toBe('outline');
		const rules = matchingStyles(contextCard);
		expect(rules.some((rule) => /repeating-linear-gradient/.test(rule))).toBe(false);
		expect(rules.some((rule) => /border:\s*1px dashed/.test(rule))).toBe(true);

		// The badge wraps inside the card rather than running past its edge.
		const markerStyle = getComputedStyle(marker);
		expect(markerStyle.whiteSpace).toBe('normal');
		expect(markerStyle.overflowWrap).toBe('anywhere');

		// Selected work carries none of the treatment.
		expect(ordinaryCard.getAttribute('data-context-treatment')).toBeNull();
		expect(matchingStyles(ordinaryCard).some((rule) => /repeating-linear-gradient/.test(rule))).toBe(
			false
		);
	});
});

describe('layout height before measurement', () => {
	it('gives a long card a taller deterministic estimate than a short one', async () => {
		await openGraph({ trees: { combined: LONG_TREE } });
		const long = card('wm-long-root');
		const short = card('wm-long-short');

		const longHeight = Number(long.getAttribute('data-graph-height'));
		const shortHeight = Number(short.getAttribute('data-graph-height'));
		expect(shortHeight).toBe(84);
		expect(longHeight).toBeGreaterThan(shortHeight);

		// The height the layout used is the height it asked the renderer for, so
		// the wrapper a reader sees is the laid-out box rather than a guess.
		const wrapper = long.closest('.svelte-flow__node') as HTMLElement;
		expect(wrapper.style.height).toBe(`${longHeight}px`);
	});
});

/**
 * A replaceable `ResizeObserver`.
 *
 * jsdom has no layout, so a card cannot report a real height. This double keeps
 * the standard browser entry point: the card observes itself exactly as it does
 * in a real browser, and the test reports the border-box height that a browser
 * would, so the assertion is about the layout consuming a measurement rather
 * than about a component prop.
 */
class RecordingResizeObserver {
	static instances: RecordingResizeObserver[] = [];
	targets: Element[] = [];
	private readonly callback: ResizeObserverCallback;

	constructor(callback: ResizeObserverCallback) {
		this.callback = callback;
		RecordingResizeObserver.instances.push(this);
	}

	observe(target: Element): void {
		this.targets.push(target);
	}

	unobserve(target: Element): void {
		this.targets = this.targets.filter((entry) => entry !== target);
	}

	disconnect(): void {
		this.targets = [];
	}

	report(target: Element, height: number): void {
		this.callback(
			[
				{
					target,
					borderBoxSize: [{ blockSize: height, inlineSize: 216 }],
					contentRect: { width: 216, height, top: 0, left: 0, bottom: height, right: 216, x: 0, y: 0 }
				} as unknown as ResizeObserverEntry
			],
			this as unknown as ResizeObserver
		);
	}
}

describe('measured node height', () => {
	const OriginalResizeObserver = globalThis.ResizeObserver;

	afterEach(() => {
		globalThis.ResizeObserver = OriginalResizeObserver;
	});

	it('lays the hierarchy out with the height a card reports', async () => {
		globalThis.ResizeObserver = RecordingResizeObserver as unknown as typeof ResizeObserver;
		await openGraph({ trees: { combined: hierarchyTree } });

		const root = card('wm-h-root');
		const child = card('wm-h-child');
		expect(number(root, 'data-graph-height')).toBe(84);
		const childYBefore = number(child, 'data-graph-y');
		const wrapper = root.closest('.svelte-flow__node') as HTMLElement;
		expect(wrapper.style.height).toBe('84px');

		const observer = RecordingResizeObserver.instances.find((entry) =>
			entry.targets.includes(root)
		);
		expect(observer).toBeDefined();
		observer?.report(root, 200);

		// The measured height, not the estimate, is what the layout and the
		// renderer use; the child is pushed down by exactly the extra height.
		await waitFor(() => expect(number(root, 'data-graph-height')).toBe(200));
		expect(wrapper.style.height).toBe('200px');
		expect(number(child, 'data-graph-y')).toBe(childYBefore + (200 - 84));
	});
});

describe('measured height and the entry fit', () => {
	const OriginalResizeObserver = globalThis.ResizeObserver;

	afterEach(() => {
		globalThis.ResizeObserver = OriginalResizeObserver;
	});

	it('keeps a first-entry fit fitted after the cards report their real heights', async () => {
		globalThis.ResizeObserver = RecordingResizeObserver as unknown as typeof ResizeObserver;
		await openGraph({ trees: { combined: LONG_TREE } });
		const entry = viewport();
		expect(entry.fitted).toBe(true);

		const root = card('wm-long-root');
		const observer = RecordingResizeObserver.instances.find((entry) =>
			entry.targets.includes(root)
		);
		observer?.report(root, 320);

		// The fit was computed from estimates before paint. Once the real height
		// arrives the visible graph is re-fitted, so the readout does not call the
		// entry state "panned" and no taller card sits outside the canvas.
		await waitFor(() => expect(number(root, 'data-graph-height')).toBe(320));
		await waitFor(() => expect(viewport().fitted).toBe(true));
		expect(viewport().zoom).toBeLessThan(entry.zoom);
	});
});
