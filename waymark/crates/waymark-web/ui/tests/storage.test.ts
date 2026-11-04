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
		// The category scope belongs to the project, not to one tree view: the
		// grouped tree and the graph filter by the same selection.
		expect(preferenceKey(namespace, 'categories')).toBe(`${namespace}:categories`);
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
	it('defaults to the combined view, everything expanded, and no overlay', () => {
		const store = new PreferenceStore(memoryStorage(), projectIdFor('/projects/alpha'));
		expect(store.readView()).toBe('combined');
		expect(store.readExpanded('combined')).toBeNull();
		expect(store.readDependencies('combined')).toBe(false);
		expect(store.readScroll('combined')).toBe(0);
		expect(store.readCategories()).toBeNull();
		expect(store.failed).toBe(false);
	});

	it('round-trips each preference, per view', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		store.writeView('grouped');
		store.writeExpanded('combined', ['a', 'b']);
		store.writeDependencies('combined', true);
		store.writeScroll('combined', 321.6);
		store.writeCategories(['bug', 'task']);

		const reopened = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		expect(reopened.readView()).toBe('grouped');
		expect(reopened.readExpanded('combined')).toEqual(['a', 'b']);
		expect(reopened.readExpanded('grouped')).toBeNull();
		expect(reopened.readDependencies('combined')).toBe(true);
		expect(reopened.readDependencies('grouped')).toBe(false);
		expect(reopened.readScroll('combined')).toBe(322);
		expect(reopened.readCategories()).toEqual(['bug', 'task']);
		expect(reopened.failed).toBe(false);
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
		alpha.writeDependencies('combined', true);
		alpha.writeCategories(['bug']);

		expect(beta.readView()).toBe('combined');
		expect(beta.readDependencies('combined')).toBe(false);
		expect(beta.readCategories()).toBeNull();
		expect(alpha.projectId).not.toBe(beta.projectId);
	});

	it('ignores unreadable stored values', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		storage.setItem(preferenceKey(store.projectId, 'view'), 'sideways');
		storage.setItem(preferenceKey(store.projectId, 'expanded', 'combined'), '{not json');
		storage.setItem(preferenceKey(store.projectId, 'scroll', 'combined'), '-4');
		storage.setItem(preferenceKey(store.projectId, 'categories'), '{"bug":true}');
		expect(store.readView()).toBe('combined');
		expect(store.readExpanded('combined')).toBeNull();
		expect(store.readScroll('combined')).toBe(0);
		expect(store.readCategories()).toBeNull();
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
		expect(store.readCategories()).toBeNull();
		expect(() => store.writeView('grouped')).not.toThrow();
		expect(() => store.writeCategories(['bug'])).not.toThrow();
		expect(store.readView()).toBe('combined');
		expect(store.readCategories()).toBeNull();
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
