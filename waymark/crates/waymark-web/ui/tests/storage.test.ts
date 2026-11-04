import { describe, expect, it } from 'vitest';

import { normalizeProjectRoot, PreferenceStore, preferenceKey, projectIdFor } from '$lib/storage';
import type { ViewName } from '$lib/types';

function memoryStorage(): Storage {
	const values = new Map<string, string>();
	return {
		get length() {
			return values.size;
		},
		clear: () => values.clear(),
		getItem: (key: string) => values.get(key) ?? null,
		key: (index: number) => Array.from(values.keys())[index] ?? null,
		removeItem: (key: string) => {
			values.delete(key);
		},
		setItem: (key: string, value: string) => {
			values.set(key, value);
		}
	} as unknown as Storage;
}

/** The keys a `Storage` holds, through its public interface. */
function storedKeys(storage: Storage): string[] {
	return Array.from({ length: storage.length }, (_, index) => storage.key(index) ?? '').sort();
}

describe('project identifier', () => {
	it('normalizes path spelling without collapsing distinct roots', () => {
		expect(normalizeProjectRoot('/a/b/')).toBe('/a/b');
		expect(normalizeProjectRoot('/a//b///')).toBe('/a/b');
		expect(normalizeProjectRoot('C:\\work\\project\\')).toBe('C:/work/project');
		expect(normalizeProjectRoot('')).toBe('/');
		expect(normalizeProjectRoot('/')).toBe('/');
		expect(normalizeProjectRoot('/a/b')).not.toBe(normalizeProjectRoot('/a/c'));
	});

	it('derives one stable, path-free id per normalized root', () => {
		const id = projectIdFor('/projects/alpha');
		expect(projectIdFor('/projects/alpha/')).toBe(id);
		expect(projectIdFor('/projects//alpha')).toBe(id);
		expect(projectIdFor('/projects/beta')).not.toBe(id);
		expect(id).not.toContain('alpha');
		expect(id.startsWith('waymark.web.v1:')).toBe(true);
	});

	it('builds preference keys inside the project namespace', () => {
		const namespace = projectIdFor('/projects/alpha');
		expect(preferenceKey(namespace, 'view')).toBe(`${namespace}:view`);
		expect(preferenceKey(namespace, 'expanded', 'grouped')).toBe(`${namespace}:expanded:grouped`);
		// The presentation is one choice per project, independent of the tree view.
		expect(preferenceKey(namespace, 'presentation')).toBe(`${namespace}:presentation`);
		// The category scope belongs to the project, not to one tree view: the
		// grouped tree and the graph filter by the same selection.
		expect(preferenceKey(namespace, 'categories')).toBe(`${namespace}:categories`);
		// The graph's own state is never stored under the tree's keys.
		expect(preferenceKey(namespace, 'collapsed')).toBe(`${namespace}:collapsed`);
		expect(preferenceKey(namespace, 'viewport')).toBe(`${namespace}:viewport`);
		expect(preferenceKey(namespace, 'expanded', 'combined')).not.toBe(
			preferenceKey(namespace, 'collapsed')
		);
		expect(preferenceKey(namespace, 'scroll', 'combined')).not.toBe(
			preferenceKey(namespace, 'viewport')
		);
		// The graph's text alternative is one choice per project too, on its own
		// key so it cannot collide with the graph's collapse or viewport state.
		expect(preferenceKey(namespace, 'graphAlternative')).toBe(
			`${namespace}:graphAlternative`
		);
	});

	it('refuses a raw project root where a project id is required', () => {
		// Compile-time guard: `npm run check` fails if this call is accepted, so
		// the key can never be built from the un-namespaced root stage.
		// @ts-expect-error a project root must be turned into a ProjectId first
		const unnamespaced: string = preferenceKey('/projects/alpha', 'view');
		// No prefix sniffing either: whatever is passed is used verbatim.
		expect(unnamespaced).toBe('/projects/alpha:view');
	});
});

describe('PreferenceStore', () => {
	it('defaults to the combined view, everything expanded, no overlay, and the tree presentation', () => {
		const store = new PreferenceStore(memoryStorage(), projectIdFor('/projects/alpha'));
		expect(store.readView()).toBe('combined');
		expect(store.readPresentation()).toBe('tree');
		expect(store.readExpanded('combined')).toBeNull();
		expect(store.readDependencies('combined')).toBe(false);
		expect(store.readScroll('combined')).toBe(0);
		expect(store.readCategories()).toBeNull();
		expect(store.readGraphAlternative()).toBe(false);
		expect(store.failed).toBe(false);
	});

	it('round-trips each preference, per view', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		store.writeView('grouped');
		store.writePresentation('graph');
		store.writeExpanded('combined', ['a', 'b']);
		store.writeDependencies('combined', true);
		store.writeScroll('combined', 321.6);
		store.writeCategories(['bug', 'task']);

		const reopened = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		expect(reopened.readView()).toBe('grouped');
		expect(reopened.readPresentation()).toBe('graph');
		expect(reopened.readExpanded('combined')).toEqual(['a', 'b']);
		expect(reopened.readExpanded('grouped')).toBeNull();
		expect(reopened.readDependencies('combined')).toBe(true);
		expect(reopened.readDependencies('grouped')).toBe(false);
		expect(reopened.readScroll('combined')).toBe(322);
		expect(reopened.readCategories()).toEqual(['bug', 'task']);
		expect(reopened.failed).toBe(false);
	});

	it('round-trips the graph text alternative, and defaults to the drawing', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		store.writeGraphAlternative(true);
		const reopened = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		expect(reopened.readGraphAlternative()).toBe(true);
		expect(storage.getItem(preferenceKey(store.projectId, 'graphAlternative'))).toBe('true');

		reopened.writeGraphAlternative(false);
		expect(new PreferenceStore(storage, projectIdFor('/projects/alpha')).readGraphAlternative()).toBe(
			false
		);
	});

	it('ignores an unreadable stored text-alternative choice', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		storage.setItem(preferenceKey(store.projectId, 'graphAlternative'), 'sideways');
		expect(store.readGraphAlternative()).toBe(false);
	});

	it('keeps the presentation independent of the tree view', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		store.writeView('grouped');
		store.writePresentation('graph');

		const reopened = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		// The tree view preference is still `grouped`, still stored under its own
		// key, and switching views later does not touch the presentation.
		expect(reopened.readView()).toBe('grouped');
		reopened.writeView('combined');
		expect(reopened.readView()).toBe('combined');
		expect(reopened.readPresentation()).toBe('graph');
		expect(storage.getItem(preferenceKey(store.projectId, 'presentation'))).toBe('graph');
	});

	it('remembers the graph collapse set and viewport, apart from tree state', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		store.writeCollapsed(['wm-h-root']);
		store.writeViewport({ x: 12.5, y: -3, zoom: 0.75 });
		store.writeExpanded('combined', ['wm-h-child']);
		store.writeScroll('combined', 140);

		const reopened = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		expect(reopened.readCollapsed()).toEqual(['wm-h-root']);
		expect(reopened.readViewport()).toEqual({ x: 12.5, y: -3, zoom: 0.75 });
		// The tree's own expansion and scroll are untouched by the graph.
		expect(reopened.readExpanded('combined')).toEqual(['wm-h-child']);
		expect(reopened.readScroll('combined')).toBe(140);
		expect(reopened.readCollapsed()).not.toEqual(reopened.readExpanded('combined'));
	});

	it('remembers an empty category selection as a real choice', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		store.writeCategories([]);
		const reopened = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		// `null` means "nothing stored, show everything"; `[]` means "show none".
		expect(reopened.readCategories()).toEqual([]);
	});

	it('isolates two projects sharing one origin', () => {
		const storage = memoryStorage();
		const alpha = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		const beta = new PreferenceStore(storage, projectIdFor('/projects/beta'));
		alpha.writeView('grouped');
		alpha.writePresentation('graph');
		alpha.writeDependencies('combined', true);
		alpha.writeCategories(['bug']);

		expect(beta.readView()).toBe('combined');
		expect(beta.readPresentation()).toBe('tree');
		expect(beta.readDependencies('combined')).toBe(false);
		expect(beta.readCategories()).toBeNull();
		expect(beta.readCollapsed()).toBeNull();
		expect(beta.readViewport()).toBeNull();
		expect(beta.readGraphAlternative()).toBe(false);
		expect(alpha.projectId).not.toBe(beta.projectId);
	});

	it('ignores unreadable stored values', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		storage.setItem(preferenceKey(store.projectId, 'view'), 'sideways');
		storage.setItem(preferenceKey(store.projectId, 'presentation'), 'diagram');
		storage.setItem(preferenceKey(store.projectId, 'expanded', 'combined'), '{not json');
		storage.setItem(preferenceKey(store.projectId, 'scroll', 'combined'), '-4');
		storage.setItem(preferenceKey(store.projectId, 'categories'), '{"bug":true}');
		storage.setItem(preferenceKey(store.projectId, 'collapsed'), '{not json');
		storage.setItem(preferenceKey(store.projectId, 'viewport'), '{"x":"left"}');
		expect(store.readView()).toBe('combined');
		expect(store.readPresentation()).toBe('tree');
		expect(store.readExpanded('combined')).toBeNull();
		expect(store.readScroll('combined')).toBe(0);
		expect(store.readCategories()).toBeNull();
		expect(store.readCollapsed()).toBeNull();
		expect(store.readViewport()).toBeNull();
	});

	it('drops non-string entries from a malformed category scope', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		storage.setItem(preferenceKey(store.projectId, 'categories'), '["bug",7,null,"task"]');
		// Unknown category keys are kept here and dropped when the scope is
		// resolved against this project's configured categories.
		expect(store.readCategories()).toEqual(['bug', 'task']);
	});

	it('degrades without throwing when storage is unavailable', () => {
		const store = new PreferenceStore(null, projectIdFor('/projects/alpha'));
		expect(store.failed).toBe(true);
		expect(store.readView()).toBe('combined');
		expect(store.readPresentation()).toBe('tree');
		expect(store.readCategories()).toBeNull();
		expect(store.readCollapsed()).toBeNull();
		expect(store.readViewport()).toBeNull();
		expect(() => store.writeView('grouped')).not.toThrow();
		expect(() => store.writePresentation('graph')).not.toThrow();
		expect(() => store.writeCategories(['bug'])).not.toThrow();
		expect(() => store.writeCollapsed(['wm-h-root'])).not.toThrow();
		expect(() => store.writeViewport({ x: 10, y: 20, zoom: 1.5 })).not.toThrow();
		expect(() => store.writeGraphAlternative(true)).not.toThrow();
		expect(store.readGraphAlternative()).toBe(false);
		expect(store.readView()).toBe('combined');
		expect(store.readCategories()).toBeNull();
		expect(store.readCollapsed()).toBeNull();
		expect(store.readViewport()).toBeNull();
	});

	it('reports a write failure instead of throwing', () => {
		const storage = memoryStorage();
		storage.setItem = () => {
			throw new Error('quota exceeded');
		};
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		expect(() => store.writeDependencies('combined', true)).not.toThrow();
		expect(store.failed).toBe(true);
	});
});

/**
 * The remembered preferences, as one table, so a per-preference test cannot
 * silently skip one and a preference written to another preference's key fails
 * instead of round-tripping by luck.
 *
 * `raw` is the exact stored string, and `view` is the data view the preference
 * is keyed by (`undefined` for the per-project ones).
 */
interface PreferenceCase {
	name: Parameters<typeof preferenceKey>[1];
	view?: ViewName;
	write: (store: PreferenceStore) => void;
	read: (store: PreferenceStore) => unknown;
	expected: unknown;
	raw: string;
	defaulted: unknown;
}

const PREFERENCES: PreferenceCase[] = [
	{
		name: 'view',
		write: (store) => store.writeView('grouped'),
		read: (store) => store.readView(),
		expected: 'grouped',
		raw: 'grouped',
		defaulted: 'combined'
	},
	{
		name: 'presentation',
		write: (store) => store.writePresentation('graph'),
		read: (store) => store.readPresentation(),
		expected: 'graph',
		raw: 'graph',
		defaulted: 'tree'
	},
	{
		name: 'graphAlternative',
		write: (store) => store.writeGraphAlternative(true),
		read: (store) => store.readGraphAlternative(),
		expected: true,
		raw: 'true',
		defaulted: false
	},
	{
		name: 'categories',
		write: (store) => store.writeCategories(['bug', 'task']),
		read: (store) => store.readCategories(),
		expected: ['bug', 'task'],
		raw: JSON.stringify(['bug', 'task']),
		defaulted: null
	},
	{
		name: 'expanded',
		view: 'grouped',
		write: (store) => store.writeExpanded('grouped', ['wm-h-root']),
		read: (store) => store.readExpanded('grouped'),
		expected: ['wm-h-root'],
		raw: JSON.stringify(['wm-h-root']),
		defaulted: null
	},
	{
		name: 'dependencies',
		view: 'grouped',
		write: (store) => store.writeDependencies('grouped', true),
		read: (store) => store.readDependencies('grouped'),
		expected: true,
		raw: 'true',
		defaulted: false
	},
	{
		name: 'collapsed',
		write: (store) => store.writeCollapsed(['wm-h-child']),
		read: (store) => store.readCollapsed(),
		expected: ['wm-h-child'],
		raw: JSON.stringify(['wm-h-child']),
		defaulted: null
	},
	{
		name: 'viewport',
		write: (store) => store.writeViewport({ x: 12.5, y: -3, zoom: 0.75 }),
		read: (store) => store.readViewport(),
		expected: { x: 12.5, y: -3, zoom: 0.75 },
		raw: JSON.stringify({ x: 12.5, y: -3, zoom: 0.75 }),
		defaulted: null
	},
	{
		name: 'scroll',
		view: 'grouped',
		write: (store) => store.writeScroll('grouped', 320),
		read: (store) => store.readScroll('grouped'),
		expected: 320,
		raw: '320',
		defaulted: 0
	}
];

describe('every remembered preference', () => {
	for (const preference of PREFERENCES) {
		it(`round-trips ${preference.name} under its own key`, () => {
			const storage = memoryStorage();
			const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
			preference.write(store);

			// One write, one key: the value is where `preferenceKey` says it is.
			const key = preferenceKey(store.projectId, preference.name, preference.view);
			expect(storedKeys(storage)).toEqual([key]);
			expect(storage.getItem(key)).toBe(preference.raw);

			const reopened = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
			expect(preference.read(reopened)).toEqual(preference.expected);
		});

		it(`reads the default for a project that never wrote ${preference.name}`, () => {
			const storage = memoryStorage();
			const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
			expect(preference.read(store)).toEqual(preference.defaulted);
		});

		it(`keeps ${preference.name} out of another project`, () => {
			const storage = memoryStorage();
			const alpha = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
			const beta = new PreferenceStore(storage, projectIdFor('/projects/beta'));
			preference.write(alpha);
			expect(preference.read(beta)).toEqual(preference.defaulted);
		});

		it(`namespaces ${preference.name} by the normalized project root`, () => {
			const storage = memoryStorage();
			const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
			preference.write(store);
			// The same root, spelled differently, is the same project.
			const spelled = new PreferenceStore(storage, projectIdFor('/projects//alpha/'));
			expect(preference.read(spelled)).toEqual(preference.expected);
			// A sibling directory is not.
			const sibling = new PreferenceStore(storage, projectIdFor('/projects/alphabeta'));
			expect(preference.read(sibling)).toEqual(preference.defaulted);
		});
	}
});

describe('per-view preferences keep their view apart', () => {
	it('stores expansion, dependency visibility, and scroll per data view', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		store.writeExpanded('combined', ['wm-h-child']);
		store.writeDependencies('combined', true);
		store.writeScroll('combined', 90);

		// The other view is untouched by those writes.
		expect(store.readExpanded('grouped')).toBeNull();
		expect(store.readDependencies('grouped')).toBe(false);
		expect(store.readScroll('grouped')).toBe(0);

		store.writeExpanded('grouped', ['wm-h-root']);
		store.writeDependencies('grouped', true);
		store.writeScroll('grouped', 240);

		// And writing the second view does not rewrite the first.
		expect(store.readExpanded('combined')).toEqual(['wm-h-child']);
		expect(store.readDependencies('combined')).toBe(true);
		expect(store.readScroll('combined')).toBe(90);
		expect(store.readExpanded('grouped')).toEqual(['wm-h-root']);
		expect(store.readScroll('grouped')).toBe(240);
	});

	it('keeps per-project preferences shared by both data views', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		store.writePresentation('graph');
		store.writeCategories(['bug']);
		store.writeCollapsed(['wm-h-child']);
		store.writeViewport({ x: 1, y: 2, zoom: 1.5 });
		store.writeGraphAlternative(true);

		// Switching the data view reads the same project-scoped values.
		store.writeView('grouped');
		const reopened = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		expect(reopened.readView()).toBe('grouped');
		expect(reopened.readPresentation()).toBe('graph');
		expect(reopened.readCategories()).toEqual(['bug']);
		expect(reopened.readCollapsed()).toEqual(['wm-h-child']);
		expect(reopened.readViewport()).toEqual({ x: 1, y: 2, zoom: 1.5 });
		expect(reopened.readGraphAlternative()).toBe(true);
	});

	it('never writes one preference while reading another', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		for (const preference of PREFERENCES) preference.read(store);
		expect(storedKeys(storage)).toEqual([]);
		expect(store.failed).toBe(false);
	});
});

describe('project identity across spellings and moves', () => {
	it('treats a dot segment as the same project as the canonical path', () => {
		const canonical = projectIdFor('/a/b');
		expect(projectIdFor('/a/b/')).toBe(canonical);
		expect(projectIdFor('/a//b')).toBe(canonical);
		expect(projectIdFor('/a/./b')).toBe(canonical);
		expect(projectIdFor('/a/b/../b')).toBe(canonical);
		expect(projectIdFor('/a/b/.')).toBe(canonical);
		expect(projectIdFor('/a/c/../b/')).toBe(canonical);
		expect(normalizeProjectRoot('/a/./b')).toBe('/a/b');
		expect(normalizeProjectRoot('/a/b/../c')).toBe('/a/c');
		expect(normalizeProjectRoot('/a/..')).toBe('/');
		expect(normalizeProjectRoot('.')).toBe('/');
		expect(normalizeProjectRoot('C:\\work\\.\\project')).toBe('C:/work/project');
	});

	it('starts a moved project from defaults and leaves the old namespace intact', () => {
		const storage = memoryStorage();
		const before = new PreferenceStore(storage, projectIdFor('/work/alpha'));
		before.writeView('grouped');
		before.writePresentation('graph');
		before.writeGraphAlternative(true);
		before.writeCategories(['bug']);
		before.writeExpanded('combined', ['wm-h-root']);
		before.writeDependencies('combined', true);
		before.writeCollapsed(['wm-h-child']);
		before.writeViewport({ x: 10, y: 20, zoom: 1.25 });
		before.writeScroll('combined', 240);

		const moved = new PreferenceStore(storage, projectIdFor('/archive/alpha'));
		expect(moved.projectId).not.toBe(before.projectId);
		expect(moved.readView()).toBe('combined');
		expect(moved.readPresentation()).toBe('tree');
		expect(moved.readGraphAlternative()).toBe(false);
		expect(moved.readCategories()).toBeNull();
		expect(moved.readExpanded('combined')).toBeNull();
		expect(moved.readDependencies('combined')).toBe(false);
		expect(moved.readCollapsed()).toBeNull();
		expect(moved.readViewport()).toBeNull();
		expect(moved.readScroll('combined')).toBe(0);

		// The old namespace is still there, untouched by the move.
		const still = new PreferenceStore(storage, projectIdFor('/work/alpha/'));
		expect(still.readView()).toBe('grouped');
		expect(still.readPresentation()).toBe('graph');
		expect(still.readGraphAlternative()).toBe(true);
		expect(still.readCategories()).toEqual(['bug']);
		expect(still.readExpanded('combined')).toEqual(['wm-h-root']);
		expect(still.readDependencies('combined')).toBe(true);
		expect(still.readCollapsed()).toEqual(['wm-h-child']);
		expect(still.readViewport()).toEqual({ x: 10, y: 20, zoom: 1.25 });
		expect(still.readScroll('combined')).toBe(240);
	});
});

describe('malformed stored values', () => {
	it('keeps a fitted viewport below the user zoom floor', () => {
		// A large graph fits below 0.5; that is a legitimate remembered view, not
		// a malformed value, so reopening the project must restore it.
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		storage.setItem(
			preferenceKey(store.projectId, 'viewport'),
			JSON.stringify({ x: 12, y: 8, zoom: 0.3 })
		);
		expect(store.readViewport()).toEqual({ x: 12, y: 8, zoom: 0.3 });
	});

	it('falls back to the default for every parser', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		const malformed: Array<[Parameters<typeof preferenceKey>[1], string, string]> = [
			['view', 'sideways', 'combined'],
			['presentation', 'diagram', 'tree'],
			['graphAlternative', 'sideways', 'false'],
			['categories', '{"bug":true}', 'null'],
			['expanded', '{not json', 'null'],
			['expanded', '"wm-h-root"', 'null'],
			['expanded', '42', 'null'],
			['dependencies', 'yes', 'false'],
			['collapsed', '{not json', 'null'],
			['viewport', '{"x":"left"}', 'null'],
			['viewport', '{"x":1,"y":2,"zoom":9}', 'null'],
			['viewport', '{"x":1,"y":2,"zoom":0}', 'null'],
			['scroll', '-4', '0'],
			['scroll', 'Infinity', '0']
		];
		for (const [preference, raw] of malformed) {
			storage.setItem(preferenceKey(store.projectId, preference), raw);
		}

		expect(store.readView()).toBe('combined');
		expect(store.readPresentation()).toBe('tree');
		expect(store.readGraphAlternative()).toBe(false);
		expect(store.readCategories()).toBeNull();
		expect(store.readExpanded('combined')).toBeNull();
		expect(store.readDependencies('combined')).toBe(false);
		expect(store.readCollapsed()).toBeNull();
		expect(store.readViewport()).toBeNull();
		expect(store.readScroll('combined')).toBe(0);
	});

	it('treats a stored array of the wrong element type as missing, not as a choice', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		// A corrupted array is not "the reader selected nothing"; defaulting is
		// the only safe reading, and it must not throw.
		storage.setItem(preferenceKey(store.projectId, 'categories'), '[7,null,true]');
		storage.setItem(preferenceKey(store.projectId, 'expanded', 'combined'), '[7,null]');
		storage.setItem(preferenceKey(store.projectId, 'collapsed'), '[{"id":"wm-h-root"}]');
		expect(store.readCategories()).toBeNull();
		expect(store.readExpanded('combined')).toBeNull();
		expect(store.readCollapsed()).toBeNull();

		// An empty array is still a real choice and still respected.
		storage.setItem(preferenceKey(store.projectId, 'categories'), '[]');
		storage.setItem(preferenceKey(store.projectId, 'expanded', 'combined'), '[]');
		expect(store.readCategories()).toEqual([]);
		expect(store.readExpanded('combined')).toEqual([]);
	});

	it('keeps working after reading malformed values', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		storage.setItem(preferenceKey(store.projectId, 'viewport'), '{truncated');
		expect(store.readViewport()).toBeNull();
		store.writeViewport({ x: 1, y: 2, zoom: 1.5 });
		expect(new PreferenceStore(storage, store.projectId).readViewport()).toEqual({
			x: 1,
			y: 2,
			zoom: 1.5
		});
	});

	it('degrades without throwing when reads throw', () => {
		const storage = memoryStorage();
		storage.getItem = () => {
			throw new Error('access denied');
		};
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		expect(() => store.readView()).not.toThrow();
		expect(store.readView()).toBe('combined');
		expect(store.readPresentation()).toBe('tree');
		expect(store.readGraphAlternative()).toBe(false);
		expect(store.readCategories()).toBeNull();
		expect(store.readExpanded('combined')).toBeNull();
		expect(store.readDependencies('combined')).toBe(false);
		expect(store.readCollapsed()).toBeNull();
		expect(store.readViewport()).toBeNull();
		expect(store.readScroll('combined')).toBe(0);
		expect(store.failed).toBe(true);
	});

	it('reports a quota failure by name without throwing', () => {
		const storage = memoryStorage();
		storage.setItem = () => {
			const error = new Error('The quota has been exceeded.');
			error.name = 'QuotaExceededError';
			throw error;
		};
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		for (const preference of PREFERENCES) {
			expect(() => preference.write(store)).not.toThrow();
		}
		expect(store.failed).toBe(true);
	});
});
