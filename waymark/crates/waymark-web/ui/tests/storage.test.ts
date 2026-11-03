import { describe, expect, it } from 'vitest';

import { projectKey, normalizeProjectRoot, PreferenceStore, preferenceKey } from '$lib/storage';

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

	it('derives one stable, path-free key per normalized root', () => {
		const key = projectKey('/projects/alpha');
		expect(projectKey('/projects/alpha/')).toBe(key);
		expect(projectKey('/projects//alpha')).toBe(key);
		expect(projectKey('/projects/beta')).not.toBe(key);
		expect(key).not.toContain('alpha');
		expect(key.startsWith('waymark.web.v1:')).toBe(true);
	});

	it('builds preference keys inside the project namespace', () => {
		const namespace = projectKey('/projects/alpha');
		expect(preferenceKey('/projects/alpha', 'view')).toBe(`${namespace}:view`);
		expect(preferenceKey(namespace, 'expanded', 'grouped')).toBe(`${namespace}:expanded:grouped`);
	});
});

describe('PreferenceStore', () => {
	it('defaults to the combined view, everything expanded, and no overlay', () => {
		const store = new PreferenceStore(memoryStorage(), '/projects/alpha');
		expect(store.readView()).toBe('combined');
		expect(store.readExpanded('combined')).toBeNull();
		expect(store.readDependencies('combined')).toBe(false);
		expect(store.readScroll('combined')).toBe(0);
		expect(store.failed).toBe(false);
	});

	it('round-trips each preference, per view', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, '/projects/alpha');
		store.writeView('grouped');
		store.writeExpanded('combined', ['a', 'b']);
		store.writeDependencies('combined', true);
		store.writeScroll('combined', 321.6);

		const reopened = new PreferenceStore(storage, '/projects/alpha');
		expect(reopened.readView()).toBe('grouped');
		expect(reopened.readExpanded('combined')).toEqual(['a', 'b']);
		expect(reopened.readExpanded('grouped')).toBeNull();
		expect(reopened.readDependencies('combined')).toBe(true);
		expect(reopened.readDependencies('grouped')).toBe(false);
		expect(reopened.readScroll('combined')).toBe(322);
		expect(reopened.failed).toBe(false);
	});

	it('isolates two projects sharing one origin', () => {
		const storage = memoryStorage();
		const alpha = new PreferenceStore(storage, '/projects/alpha');
		const beta = new PreferenceStore(storage, '/projects/beta');
		alpha.writeView('grouped');
		alpha.writeDependencies('combined', true);

		expect(beta.readView()).toBe('combined');
		expect(beta.readDependencies('combined')).toBe(false);
		expect(alpha.projectId).not.toBe(beta.projectId);
	});

	it('ignores unreadable stored values', () => {
		const storage = memoryStorage();
		const store = new PreferenceStore(storage, '/projects/alpha');
		storage.setItem(preferenceKey(store.projectId, 'view'), 'sideways');
		storage.setItem(preferenceKey(store.projectId, 'expanded', 'combined'), '{not json');
		storage.setItem(preferenceKey(store.projectId, 'scroll', 'combined'), '-4');
		expect(store.readView()).toBe('combined');
		expect(store.readExpanded('combined')).toBeNull();
		expect(store.readScroll('combined')).toBe(0);
	});

	it('degrades without throwing when storage is unavailable', () => {
		const store = new PreferenceStore(null, '/projects/alpha');
		expect(store.failed).toBe(true);
		expect(store.readView()).toBe('combined');
		expect(() => store.writeView('grouped')).not.toThrow();
		expect(store.readView()).toBe('combined');
	});

	it('reports a write failure instead of throwing', () => {
		const storage = memoryStorage();
		storage.setItem = () => {
			throw new Error('quota exceeded');
		};
		const store = new PreferenceStore(storage, '/projects/alpha');
		expect(() => store.writeDependencies('combined', true)).not.toThrow();
		expect(store.failed).toBe(true);
	});
});
