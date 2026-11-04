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

		const reopened = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		expect(reopened.readView()).toBe('grouped');
		expect(reopened.readPresentation()).toBe('graph');
		expect(reopened.readExpanded('combined')).toEqual(['a', 'b']);
		expect(reopened.readExpanded('grouped')).toBeNull();
		expect(reopened.readDependencies('combined')).toBe(true);
		expect(reopened.readDependencies('grouped')).toBe(false);
		expect(reopened.readScroll('combined')).toBe(322);
		expect(reopened.failed).toBe(false);
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

	it('isolates two projects sharing one origin', () => {
		const storage = memoryStorage();
		const alpha = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		const beta = new PreferenceStore(storage, projectIdFor('/projects/beta'));
		alpha.writeView('grouped');
		alpha.writePresentation('graph');
		alpha.writeDependencies('combined', true);

		expect(beta.readView()).toBe('combined');
		expect(beta.readPresentation()).toBe('tree');
		expect(beta.readDependencies('combined')).toBe(false);
		expect(alpha.projectId).not.toBe(beta.projectId);
	});

	it('ignores unreadable stored values', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, projectIdFor('/projects/alpha'));
		storage.setItem(preferenceKey(store.projectId, 'view'), 'sideways');
		storage.setItem(preferenceKey(store.projectId, 'presentation'), 'diagram');
		storage.setItem(preferenceKey(store.projectId, 'expanded', 'combined'), '{not json');
		storage.setItem(preferenceKey(store.projectId, 'scroll', 'combined'), '-4');
		expect(store.readView()).toBe('combined');
		expect(store.readPresentation()).toBe('tree');
		expect(store.readExpanded('combined')).toBeNull();
		expect(store.readScroll('combined')).toBe(0);
	});

	it('degrades without throwing when storage is unavailable', () => {
		const store = new PreferenceStore(null, projectIdFor('/projects/alpha'));
		expect(store.failed).toBe(true);
		expect(store.readView()).toBe('combined');
		expect(store.readPresentation()).toBe('tree');
		expect(() => store.writeView('grouped')).not.toThrow();
		expect(() => store.writePresentation('graph')).not.toThrow();
		expect(store.readView()).toBe('combined');
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
