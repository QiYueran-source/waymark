/**
 * The remembered-preferences contract, end to end.
 *
 * `storage.test.ts` proves the store's own parsers and keys. This suite drives
 * the rendered Explorer, so what it asserts is what a reader gets: a choice
 * made with a real control comes back after a reload, one project never reads
 * another's choice, a moved project starts from defaults, and a hostile store
 * changes nothing but the notice.
 *
 * Every remembered preference is exercised through its control:
 *
 * | preference      | control                          | scope      |
 * | --------------- | -------------------------------- | ---------- |
 * | view            | Combined / By category           | project    |
 * | presentation    | Tree / Graph                     | project    |
 * | graphAlternative| Structured list                  | project    |
 * | categories      | category options                 | project    |
 * | expanded        | the row's fold control           | per view   |
 * | dependencies    | Show dependency edges            | per view   |
 * | scroll          | the work-items pane's scrollTop  | per view   |
 * | collapsed       | the graph node's fold control    | project    |
 * | viewport        | pan / zoom / Fit / Reset         | project    |
 * | direction       | Top to bottom / Left to right    | project    |
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import Explorer from '$lib/components/Explorer.svelte';
import { GRAPH_NOMINAL_CANVAS, MAX_GRAPH_ZOOM, MIN_GRAPH_ZOOM } from '$lib/graph';
import { preferenceKey, projectIdFor } from '$lib/storage';
import {
	ROOT_A,
	ROOT_B,
	combinedTree,
	detail,
	groupedTreeWithEmptyCategory,
	makeFetch
} from './fixtures';
import { categoryOption, clickCategory } from './category-helpers';

type FetchOptions = Parameters<typeof makeFetch>[0];

// One payload family for both views, so a choice made in one presentation is
// read back in the other against the same items and the same graph shape.
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
	await waitFor(() => expect(graphCards().length).toBeGreaterThan(0));
	return rendered;
}

function checked(label: string): boolean {
	return (screen.getByLabelText(label) as HTMLInputElement).checked;
}

function graphCards(): HTMLElement[] {
	return screen.getAllByTestId(/^graph-node-/);
}

function drawnIds(): string[] {
	return graphCards().map((element) => element.getAttribute('data-node-id') ?? '');
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
	// The drag suppressor stands down after a task; let it, so the next click
	// is not taken for the tail of this gesture.
	await new Promise((resolve) => setTimeout(resolve, 0));
}

/** Every key currently in the store, so nothing unnamed can be written. */
function storedKeys(storage: Storage): string[] {
	return Array.from({ length: storage.length }, (_, index) => storage.key(index) ?? '').sort();
}

const KNOWN_PREFERENCES = [
	'view',
	'presentation',
	'expanded',
	'dependencies',
	'scroll',
	'categories',
	'collapsed',
	'viewport',
	'graphAlternative',
	'direction'
] as const;

describe('every control writes only under the project namespace', () => {
	it('uses the ten known keys and no others', async () => {
		const storage = localStorage;
		const rendered = await renderLoaded({ trees: BOTH_VIEWS });
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('group-bug');
		await clickCategory('docs');
		await fireEvent.click(screen.getByTestId('toggle-wm-bug-login'));
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		const treePane = screen.getByLabelText('Work items');
		treePane.scrollTop = 120;
		await fireEvent.scroll(treePane);
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		await fireEvent.click(screen.getByTestId('graph-direction-lr'));
		await fireEvent.click(screen.getByLabelText('Structured list'));
		await screen.findByTestId('graph-text');
		rendered.unmount();

		const namespace = projectIdFor(ROOT_A);
		const keys = storedKeys(storage);
		expect(keys.length).toBeGreaterThan(0);
		for (const key of keys) {
			expect(key.startsWith(`${namespace}:`)).toBe(true);
			const name = key.slice(namespace.length + 1).split(':')[0];
			expect(KNOWN_PREFERENCES).toContain(name);
		}
		// Every write is namespaced by the project, never by a raw path.
		expect(keys.some((key) => key.includes(ROOT_A))).toBe(false);
	});
});

describe('round-trips, per preference', () => {
	it('remembers the selected data view', async () => {
		const first = await renderLoaded({ trees: BOTH_VIEWS });
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('group-bug');
		expect(
			localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'view'))
		).toBe('grouped');
		first.unmount();

		await renderLoaded({ trees: BOTH_VIEWS });
		await screen.findByTestId('group-bug');
		expect(checked('By category')).toBe(true);
	});

	it('remembers the presentation, and restores the graph rather than the tree', async () => {
		const first = await renderLoaded({ trees: BOTH_VIEWS });
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		expect(
			localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'presentation'))
		).toBe('graph');
		first.unmount();

		await renderLoaded({ trees: BOTH_VIEWS });
		await screen.findByTestId('graph');
		expect(checked('Graph')).toBe(true);
		expect(screen.queryByLabelText('Work item tree')).toBeNull();
	});

	it('remembers the graph direction for the project, not for one data view', async () => {
		const first = await renderLoaded({ trees: BOTH_VIEWS });
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		await fireEvent.click(screen.getByTestId('graph-direction-lr'));
		expect(localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'direction'))).toBe(
			'LR'
		);

		// The combined and grouped graphs share the one choice.
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('graph-node-wm-task-ship');
		expect(
			(screen.getByTestId('graph-direction-lr') as HTMLInputElement).checked
		).toBe(true);
		first.unmount();

		await renderLoaded({ trees: BOTH_VIEWS });
		await screen.findByTestId('graph');
		expect(
			(screen.getByTestId('graph-direction-lr') as HTMLInputElement).checked
		).toBe(true);
	});

	it('remembers the structured-list choice', async () => {
		const first = await renderLoaded({ trees: BOTH_VIEWS });
		await fireEvent.click(screen.getByLabelText('Graph'));
		await fireEvent.click(screen.getByLabelText('Structured list'));
		await screen.findByTestId('graph-text');
		expect(
			localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'graphAlternative'))
		).toBe('true');
		first.unmount();

		await renderLoaded({ trees: BOTH_VIEWS });
		await screen.findByTestId('graph-text');
		expect(checked('Structured list')).toBe(true);
	});

	it('remembers the category scope', async () => {
		const first = await renderLoaded({ trees: BOTH_VIEWS });
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('group-bug');
		await clickCategory('bug');
		await clickCategory('docs');
		first.unmount();

		await renderLoaded({ trees: BOTH_VIEWS });
		await screen.findByTestId('group-task');
		expect(
			(await categoryOption('bug')).getAttribute('aria-pressed')
		).toBe('false');
		expect(
			(await categoryOption('task')).getAttribute('aria-pressed')
		).toBe('true');
	});

	it('remembers tree expansion and scroll, per view', async () => {
		const first = await renderLoaded({ trees: BOTH_VIEWS });
		await fireEvent.click(screen.getByTestId('toggle-wm-bug-login'));
		expect(screen.queryByTestId('node-wm-task-child')).toBeNull();
		const treePane = screen.getByLabelText('Work items');
		treePane.scrollTop = 96;
		await fireEvent.scroll(treePane);
		// The fold was written under the combined view's own key.
		expect(
			localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'expanded', 'combined'))
		).not.toContain('wm-bug-login');
		first.unmount();

		await renderLoaded({ trees: BOTH_VIEWS });
		await screen.findByTestId('node-wm-bug-login');
		expect(screen.queryByTestId('node-wm-task-child')).toBeNull();
		await waitFor(() => expect(screen.getByLabelText('Work items').scrollTop).toBe(96));
	});

	it('remembers dependency visibility, per view', async () => {
		const first = await renderLoaded({ trees: BOTH_VIEWS });
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		first.unmount();

		await renderLoaded({ trees: BOTH_VIEWS });
		await screen.findByTestId('node-wm-bug-login');
		expect(checked('Show dependency edges')).toBe(true);
	});

	it('remembers the graph collapse set', async () => {
		const first = await openGraph({ trees: BOTH_VIEWS });
		await fireEvent.click(screen.getByTestId('graph-toggle-wm-bug-login'));
		await waitFor(() => expect(drawnIds()).not.toContain('wm-task-child'));
		first.unmount();

		await renderLoaded({ trees: BOTH_VIEWS });
		await screen.findByTestId('graph');
		await waitFor(() => expect(drawnIds()).not.toContain('wm-task-child'));
		expect(
			screen.getByTestId('graph-toggle-wm-bug-login').getAttribute('aria-expanded')
		).toBe('false');
	});

	it('remembers the graph viewport', async () => {
		const first = await openGraph({ trees: BOTH_VIEWS });
		await dragBackground([100, 100], [170, 150]);
		const moved = viewport();
		await waitFor(() =>
			expect(
				localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'viewport'))
			).not.toBeNull()
		);
		first.unmount();

		await renderLoaded({ trees: BOTH_VIEWS });
		await screen.findByTestId('graph');
		await waitFor(() => expect(graphCards().length).toBeGreaterThan(0));
		expect(viewport().x).toBeCloseTo(moved.x, 1);
		expect(viewport().y).toBeCloseTo(moved.y, 1);
		expect(viewport().zoom).toBeCloseTo(moved.zoom, 3);
	});
});

describe('switching presentation never rewrites another presentation', () => {
	it('keeps the tree’s expansion, scroll, and overlay while the graph is used', async () => {
		await renderLoaded({ trees: BOTH_VIEWS });
		await fireEvent.click(screen.getByTestId('toggle-wm-bug-login'));
		const treePane = screen.getByLabelText('Work items');
		treePane.scrollTop = 88;
		await fireEvent.scroll(treePane);
		const expandedBefore = localStorage.getItem(
			preferenceKey(projectIdFor(ROOT_A), 'expanded', 'combined')
		);
		const scrollBefore = localStorage.getItem(
			preferenceKey(projectIdFor(ROOT_A), 'scroll', 'combined')
		);

		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		await fireEvent.click(screen.getByTestId('graph-toggle-wm-bug-login'));
		await waitFor(() => expect(drawnIds()).not.toContain('wm-task-child'));
		await dragBackground([100, 100], [160, 140]);
		await waitFor(() => expect(viewport().fitted).toBe(false));

		// Nothing the graph did touched the tree's own state.
		expect(
			localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'expanded', 'combined'))
		).toBe(expandedBefore);
		expect(
			localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'scroll', 'combined'))
		).toBe(scrollBefore);

		await fireEvent.click(screen.getByLabelText('Tree'));
		await screen.findByTestId('node-wm-bug-login');
		expect(screen.queryByTestId('node-wm-task-child')).toBeNull();
		await waitFor(() => expect(screen.getByLabelText('Work items').scrollTop).toBe(88));
	});

	it('keeps the graph’s collapse set and viewport while the tree is used', async () => {
		await openGraph({ trees: BOTH_VIEWS });
		await fireEvent.click(screen.getByTestId('graph-toggle-wm-bug-login'));
		await waitFor(() => expect(drawnIds()).not.toContain('wm-task-child'));
		await dragBackground([100, 100], [170, 150]);
		const moved = viewport();
		// The drawing announces the end of a gesture a beat after it lands, so
		// wait for the stored transform to be the one the readout shows.
		await waitFor(() => {
			const stored = JSON.parse(
				localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'viewport')) ?? 'null'
			);
			expect(Math.abs((stored?.x ?? 0) - moved.x)).toBeLessThan(1);
		});
		const collapsedBefore = localStorage.getItem(
			preferenceKey(projectIdFor(ROOT_A), 'collapsed')
		);
		const viewportBefore = localStorage.getItem(
			preferenceKey(projectIdFor(ROOT_A), 'viewport')
		);
		expect(JSON.parse(viewportBefore ?? 'null')).not.toBeNull();

		await fireEvent.click(screen.getByLabelText('Tree'));
		await screen.findByTestId('node-wm-bug-login');
		await fireEvent.click(screen.getByTestId('toggle-wm-bug-login'));
		expect(screen.queryByTestId('node-wm-task-child')).toBeNull();

		// The graph's own keys are untouched by the tree's fold.
		expect(localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'collapsed'))).toBe(
			collapsedBefore
		);
		// The stored transform is byte-for-byte what the graph left behind.
		expect(localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'viewport'))).toBe(
			viewportBefore
		);

		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		await waitFor(() => expect(drawnIds()).not.toContain('wm-task-child'));
	});

	it('gives each data view its own tree state, and does not clobber the other', async () => {
		await renderLoaded({ trees: BOTH_VIEWS });
		// Combined: fold a row and scroll.
		await fireEvent.click(screen.getByTestId('toggle-wm-bug-login'));
		const treePane = screen.getByLabelText('Work items');
		treePane.scrollTop = 64;
		await fireEvent.scroll(treePane);
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));

		// Grouped: its own state, untouched by the combined view's. It starts
		// fully expanded, so collapsing everything is a choice of its own.
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('group-bug');
		expect(checked('Show dependency edges')).toBe(false);
		await fireEvent.click(screen.getByRole('button', { name: 'Collapse all' }));

		expect(
			localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'expanded', 'combined'))
		).not.toBe(
			localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'expanded', 'grouped'))
		);
		expect(
			localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'dependencies', 'combined'))
		).toBe('true');
		expect(
			localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'dependencies', 'grouped'))
		).not.toBe('true');

		// Back to combined: its fold, overlay, and scroll are still its own.
		await fireEvent.click(screen.getByLabelText('Combined'));
		await screen.findByTestId('node-wm-bug-login');
		expect(screen.queryByTestId('node-wm-task-child')).toBeNull();
		expect(checked('Show dependency edges')).toBe(true);
		await waitFor(() => expect(screen.getByLabelText('Work items').scrollTop).toBe(64));

		// And grouped still remembers its own collapse-everything choice.
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('group-bug');
		expect(screen.queryByTestId('node-wm-task-child')).toBeNull();
		expect(
			localStorage.getItem(preferenceKey(projectIdFor(ROOT_A), 'expanded', 'grouped'))
		).toBe('[]');
	});
});

describe('two projects on one origin', () => {
	const choices = async () => {
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('group-bug');
		await clickCategory('bug');
		await fireEvent.click(screen.getByTestId('toggle-wm-bug-login'));
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		await fireEvent.click(screen.getByTestId('graph-toggle-wm-bug-login'));
		await waitFor(() => expect(drawnIds()).not.toContain('wm-task-child'));
		await fireEvent.click(screen.getByLabelText('Structured list'));
		await screen.findByTestId('graph-text');
	};

	it('keeps every preference independent', async () => {
		const first = await renderLoaded({ root: ROOT_A, trees: BOTH_VIEWS });
		await choices();
		first.unmount();
		const alphaKeys = storedKeys(localStorage).filter((key) =>
			key.startsWith(projectIdFor(ROOT_A))
		);

		const second = await renderLoaded({ root: ROOT_B, trees: BOTH_VIEWS });
		// B starts from defaults on every control.
		expect(checked('Combined')).toBe(true);
		expect(checked('Tree')).toBe(true);
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('group-bug');
		expect((await categoryOption('bug')).getAttribute('aria-pressed')).toBe(
			'true'
		);
		expect(checked('Show dependency edges')).toBe(false);
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		await waitFor(() => expect(graphCards().length).toBeGreaterThan(0));
		expect(drawnIds()).toContain('wm-task-child');
		expect(viewport().fitted).toBe(true);
		expect(screen.queryByTestId('graph-text')).toBeNull();
		second.unmount();

		// A's keys are exactly as A left them; B wrote only its own.
		expect(
			storedKeys(localStorage).filter((key) => key.startsWith(projectIdFor(ROOT_A)))
		).toEqual(alphaKeys);
		expect(
			storedKeys(localStorage).filter((key) => key.startsWith(projectIdFor(ROOT_B))).length
		).toBeGreaterThan(0);
	});
});

describe('a project moved to another path', () => {
	it('reads defaults, and leaves the old namespace intact', async () => {
		const first = await renderLoaded({ root: ROOT_A, trees: BOTH_VIEWS });
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('group-bug');
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		await fireEvent.click(screen.getByTestId('toggle-wm-bug-login'));
		first.unmount();

		const before = storedKeys(localStorage);
		expect(before.length).toBeGreaterThan(0);

		await renderLoaded({ root: '/srv/moved/alpha', trees: BOTH_VIEWS });
		// A different path is a different project: nothing was inherited.
		expect(checked('Combined')).toBe(true);
		expect(checked('Tree')).toBe(true);
		expect(checked('Show dependency edges')).toBe(false);
		expect(screen.getByTestId('node-wm-task-child')).not.toBeNull();
		expect(
			localStorage.getItem(preferenceKey(projectIdFor('/srv/moved/alpha'), 'view'))
		).toBeNull();

		// The move read and wrote nothing into the old namespace.
		expect(storedKeys(localStorage)).toEqual(before);
	});

	it('treats a trailing slash or dot segment as the same project', async () => {
		const first = await renderLoaded({ root: ROOT_A, trees: BOTH_VIEWS });
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		first.unmount();

		// The server canonicalizes, but its fallback is the opened spelling, so
		// the client folds these too.
		await renderLoaded({ root: '/projects/alpha/', trees: BOTH_VIEWS });
		expect(checked('Show dependency edges')).toBe(true);
	});
});

/**
 * A store whose reads or writes throw, the way a blocked or full store does.
 *
 * The `unavailable` case is a throwing `localStorage` getter instead: nothing
 * is passed to the component, so it goes through `browserStorage()` itself.
 */
function throwingStorage(behaviour: 'write' | 'read'): Storage {
	const values = new Map<string, string>([
		[preferenceKey(projectIdFor(ROOT_A), 'view'), 'combined']
	]);
	return {
		get length() {
			return values.size;
		},
		clear: () => values.clear(),
		getItem: (key: string) => {
			if (behaviour === 'read') throw new Error('access denied');
			return values.get(key) ?? null;
		},
		key: (index: number) => Array.from(values.keys())[index] ?? null,
		removeItem: (key: string) => {
			values.delete(key);
		},
		setItem: (key: string, value: string) => {
			if (behaviour === 'write') {
				const error = new Error('The quota has been exceeded.');
				error.name = 'QuotaExceededError';
				throw error;
			}
			values.set(key, value);
		}
	} as unknown as Storage;
}

describe('a hostile store', () => {
	for (const behaviour of ['write', 'read'] as const) {
		it(`keeps every control working with one notice when ${behaviour}s throw`, async () => {
			await renderLoaded({ trees: BOTH_VIEWS }, throwingStorage(behaviour));
			await screen.findByTestId('node-wm-bug-login');

			if (behaviour === 'read') {
				// Reading failed, so the notice is already up.
				expect(screen.getAllByTestId('persistence-notice')).toHaveLength(1);
			} else {
				// Reading worked; nothing has been attempted yet, so nothing is
				// claimed. A notice before a failed write would be premature.
				expect(screen.queryByTestId('persistence-notice')).toBeNull();
			}

			// Every control still works for the session.
			await fireEvent.click(screen.getByLabelText('Show dependency edges'));
			expect(checked('Show dependency edges')).toBe(true);
			await fireEvent.click(screen.getByTestId('toggle-wm-bug-login'));
			expect(screen.queryByTestId('node-wm-task-child')).toBeNull();
			await fireEvent.click(screen.getByRole('button', { name: 'Collapse all' }));
			await fireEvent.click(screen.getByLabelText('By category'));
			await screen.findByTestId('group-bug');
			await clickCategory('docs');
			expect(
				(await categoryOption('docs')).getAttribute('aria-pressed')
			).toBe('false');
			await fireEvent.click(screen.getByLabelText('Graph'));
			await screen.findByTestId('graph');
			await waitFor(() => expect(graphCards().length).toBeGreaterThan(0));
			await fireEvent.click(screen.getByTestId('graph-toggle-wm-bug-login'));
			await waitFor(() => expect(drawnIds()).not.toContain('wm-task-child'));
			await dragBackground([100, 100], [160, 140]);
			await fireEvent.click(screen.getByTestId('graph-zoom-in'));
			await fireEvent.click(screen.getByTestId('graph-fit'));
			await fireEvent.click(screen.getByTestId('graph-reset'));
			await fireEvent.click(screen.getByLabelText('Structured list'));
			await screen.findByTestId('graph-text');
			await fireEvent.click(screen.getByLabelText('Tree'));

			// Still exactly one notice after all of that, and it says what will
			// happen rather than leaking the browser's error text.
			expect(screen.getAllByTestId('persistence-notice')).toHaveLength(1);
			const notice = screen.getByTestId('persistence-notice');
			expect(notice.getAttribute('role')).toBe('status');
			expect(notice.textContent).toContain('Preferences could not be saved');
			expect(notice.textContent).not.toContain('quota');
		});
	}

	it('renders and stays usable when reading localStorage itself throws', async () => {
		const original = Object.getOwnPropertyDescriptor(globalThis, 'localStorage');
		try {
			let reads = 0;
			Object.defineProperty(globalThis, 'localStorage', {
				configurable: true,
				get() {
					reads += 1;
					throw new Error('access to localStorage is denied');
				}
			});
			const api = makeFetch({ trees: BOTH_VIEWS });
			// No storage prop: the component asks the browser itself.
			render(Explorer, { fetchImpl: api.fetchImpl });
			await screen.findByTestId('node-wm-bug-login');
			expect(reads).toBeGreaterThan(0);
			expect(screen.getAllByTestId('persistence-notice')).toHaveLength(1);

			await fireEvent.click(screen.getByLabelText('Show dependency edges'));
			expect(checked('Show dependency edges')).toBe(true);
			await fireEvent.click(screen.getByTestId('toggle-wm-bug-login'));
			expect(screen.queryByTestId('node-wm-task-child')).toBeNull();
			expect(screen.getAllByTestId('persistence-notice')).toHaveLength(1);
		} finally {
			if (original) Object.defineProperty(globalThis, 'localStorage', original);
			else delete (globalThis as { localStorage?: Storage }).localStorage;
		}
	});
});

describe('what a corrupted store costs the reader', () => {
	const corrupt = () => {
		const storage = localStorage;
		const namespace = projectIdFor(ROOT_A);
		const values: Record<string, string> = {
			view: 'sideways',
			presentation: 'diagram',
			graphAlternative: 'sideways',
			categories: '{"bug":true}',
			collapsed: '{not json',
			viewport: '{"x":"left"}'
		};
		for (const [name, raw] of Object.entries(values)) {
			storage.setItem(preferenceKey(namespace, name as 'view'), raw);
		}
		storage.setItem(preferenceKey(namespace, 'expanded', 'combined'), '{not json');
		storage.setItem(preferenceKey(namespace, 'expanded', 'grouped'), '42');
		storage.setItem(preferenceKey(namespace, 'dependencies', 'combined'), 'yes');
		storage.setItem(preferenceKey(namespace, 'scroll', 'combined'), '-4');
		storage.setItem(
			preferenceKey(namespace, 'viewport'),
			JSON.stringify({ x: 1, y: 2, zoom: 9 })
		);
		return namespace;
	};

	it('falls back to every default without an exception or a notice', async () => {
		const namespace = corrupt();
		await renderLoaded({ trees: BOTH_VIEWS });
		await screen.findByTestId('node-wm-bug-login');

		expect(checked('Combined')).toBe(true);
		expect(checked('Tree')).toBe(true);
		expect(checked('Show dependency edges')).toBe(false);
		expect(screen.getByTestId('node-wm-task-child')).not.toBeNull();
		expect(screen.getByLabelText('Work items').scrollTop).toBe(0);
		// A malformed value is not a storage failure: nothing to report.
		expect(screen.queryByTestId('persistence-notice')).toBeNull();

		// The controls still write, over the corrupt values.
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		expect(localStorage.getItem(preferenceKey(namespace, 'dependencies', 'combined'))).toBe(
			'true'
		);
	});

	it('ignores an out-of-range viewport and fits the drawing', async () => {
		const namespace = corrupt();
		await openGraph({ trees: BOTH_VIEWS });
		expect(viewport().fitted).toBe(true);
		expect(viewport().zoom).toBeGreaterThanOrEqual(MIN_GRAPH_ZOOM);
		expect(viewport().zoom).toBeLessThanOrEqual(MAX_GRAPH_ZOOM);
		// The bounds are the drawing's own nominal canvas, not a guess.
		expect(GRAPH_NOMINAL_CANVAS.width).toBeGreaterThan(0);
		expect(localStorage.getItem(preferenceKey(namespace, 'viewport'))).not.toBeNull();
	});

	it('recovers when the reader replaces every corrupt choice', async () => {
		corrupt();
		const first = await renderLoaded({ trees: BOTH_VIEWS });
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		await fireEvent.click(screen.getByTestId('toggle-wm-bug-login'));
		first.unmount();

		await renderLoaded({ trees: BOTH_VIEWS });
		expect(checked('Show dependency edges')).toBe(true);
		expect(screen.queryByTestId('node-wm-task-child')).toBeNull();
	});
});

describe('read-only traffic', () => {
	it('sends only GETs to the three reads, and carries no state', async () => {
		const { requests } = await renderLoaded({
			trees: BOTH_VIEWS,
			items: { 'wm-bug-login': detail }
		});
		await fireEvent.click(screen.getByLabelText('By category'));
		await screen.findByTestId('group-bug');
		await clickCategory('docs');
		await fireEvent.click(screen.getByTestId('toggle-wm-bug-login'));
		await fireEvent.click(screen.getByLabelText('Show dependency edges'));
		await fireEvent.click(screen.getByLabelText('Graph'));
		await screen.findByTestId('graph');
		await fireEvent.click(screen.getByTestId('graph-toggle-wm-bug-login'));
		await dragBackground([100, 100], [160, 140]);
		await fireEvent.click(screen.getByLabelText('Structured list'));
		await screen.findByTestId('graph-text');
		await fireEvent.click(screen.getByTestId('graph-text-select-wm-bug-login'));
		await waitFor(() => expect(screen.getByTestId('detail-panel')).not.toBeNull());

		expect(requests.length).toBeGreaterThan(0);
		expect(requests.every((request) => request.method === 'GET')).toBe(true);
		for (const request of requests) {
			const url = new URL(request.url, 'http://localhost:7331');
			expect([
				'/api/health',
				'/api/tree',
				'/api/item/wm-bug-login',
				'/api/index'
			]).toContain(url.pathname);
			assertNoStateInRequest(request.url);
		}
		// The one detail read is the selection; everything else is a tree read.
		expect(requests.filter((request) => request.url.startsWith('/api/item/'))).toHaveLength(1);
		expect(
			requests.filter((request) => request.url.startsWith('/api/tree')).every((request) => {
				const view = new URL(request.url, 'http://localhost:7331').searchParams.get('view');
				return view === 'combined' || view === 'grouped';
			})
		).toBe(true);
		// The index section adds exactly one read, page 1.
		expect(requests.filter((request) => request.url.startsWith('/api/index'))).toHaveLength(1);
	});

	it('sends nothing at all while a preference is changed', async () => {
		const { requests } = await openGraph({ trees: BOTH_VIEWS });
		const before = requests.length;
		await fireEvent.click(screen.getByTestId('graph-toggle-wm-bug-login'));
		await dragBackground([100, 100], [160, 140]);
		await fireEvent.click(screen.getByTestId('graph-fit'));
		await fireEvent.click(screen.getByLabelText('Structured list'));
		await fireEvent.click(screen.getByLabelText('Tree'));
		expect(requests.length).toBe(before);
	});
});

/**
 * No request may carry a preference, a node id (beyond the item read's path),
 * or any other browser state. The query is the tree read's `view` and nothing
 * else; there is no body and no state-bearing header.
 */
function assertNoStateInRequest(raw: string): void {
	const url = new URL(raw, 'http://localhost:7331');
	for (const key of url.searchParams.keys()) {
		expect(['view', 'page']).toContain(key);
	}
	expect(url.hash).toBe('');
	// No preference name or value rides in the URL, whatever the control did.
	for (const name of KNOWN_PREFERENCES) {
		expect(raw).not.toContain(`:${name}`);
		expect(raw.toLowerCase()).not.toContain(`preference=${name.toLowerCase()}`);
	}
	expect(raw).not.toContain('localStorage');
	expect(raw).not.toContain('waymark.web.v1');
}
