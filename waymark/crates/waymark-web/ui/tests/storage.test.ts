import { describe, expect, it } from 'vitest';

import { normalizeProjectRoot, PreferenceStore, preferenceKey, projectIdFor } from '$lib/storage';

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
	});});
