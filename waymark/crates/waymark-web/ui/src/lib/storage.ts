/**
 * Browser-local preferences: the only state the Web view remembers.
 *
 * Everything here stays in `localStorage` on this origin. Nothing is written to
 * the project, sent to the service, or uploaded. Keys are namespaced by a
 * [`ProjectId`] derived once from the normalized project-root path supplied by
 * `GET /api/health`, so two projects on one origin never share a preference.
 *
 * Storage is treated as optional. If it is unavailable (blocked, disabled, a
 * privacy mode) or a write fails (quota, revoked permission), [`PreferenceStore`]
 * keeps working in memory for the session and reports [`failed`] so the UI can
 * show a concise notice.
 */

import type { ViewName } from './types';

/** The preference namespace. Bump the version if the stored shape changes. */
export const STORAGE_PREFIX = 'waymark.web.v1';

/** Collapse path spelling differences that do not identify a different root. */
export function normalizeProjectRoot(root: string): string {
	let value = root.trim().replace(/\\/g, '/');
	value = value.replace(/\/{2,}/g, '/');
	if (value.length > 1) {
		value = value.replace(/\/+$/, '');
	}
	return value === '' ? '/' : value;
}

declare const projectIdBrand: unique symbol;

/**
 * The preference namespace for one project, derived once from the health
 * payload's project root.
 *
 * This is a distinct type, not a plain string: a root path and a namespaced id
 * cannot be confused, and [`preferenceKey`] accepts nothing else.
 */
export type ProjectId = string & { readonly [projectIdBrand]: 'ProjectId' };

/** A stable, non-reversible identifier for a project root. */
export function projectIdFor(root: string): ProjectId {
	return `${STORAGE_PREFIX}:${hash53(normalizeProjectRoot(root)).toString(16)}` as ProjectId;
}

/** The key for one preference of one view in one project. */
export function preferenceKey(
	projectId: ProjectId,
	preference: 'view' | 'expanded' | 'dependencies' | 'scroll',
	view?: ViewName
): string {
	return view === undefined
		? `${projectId}:${preference}`
		: `${projectId}:${preference}:${view}`;
}

/** Access `localStorage` without letting a hostile/disabled store throw. */
export function browserStorage(): Storage | null {
	try {
		return globalThis.localStorage ?? null;
	} catch {
		return null;
	}
}

function isViewName(value: string | null): value is ViewName {
	return value === 'combined' || value === 'grouped';
}

function parseStringArray(raw: string | null): string[] | null {
	if (raw === null) return null;
	try {
		const parsed: unknown = JSON.parse(raw);
		if (!Array.isArray(parsed)) return null;
		return parsed.filter((entry): entry is string => typeof entry === 'string');
	} catch {
		return null;
	}
}

function parseScroll(raw: string | null): number {
	if (raw === null) return 0;
	const value = Number(raw);
	return Number.isFinite(value) && value >= 0 ? value : 0;
}

/** The project-scoped preference store, tolerant of unavailable storage. */
export class PreferenceStore {
	readonly projectId: ProjectId;
	/** True once any read/write failed or storage was unavailable. */
	failed = false;
	private readonly storage: Storage | null;

	constructor(storage: Storage | null, projectId: ProjectId) {
		this.storage = storage;
		this.projectId = projectId;
		if (storage === null) {
			this.failed = true;
		}
	}

	private read(key: string): string | null {
		if (this.storage === null) {
			this.failed = true;
			return null;
		}
		try {
			return this.storage.getItem(key);
		} catch {
			this.failed = true;
			return null;
		}
	}

	private write(key: string, value: string): void {
		if (this.storage === null) {
			this.failed = true;
			return;
		}
		try {
			this.storage.setItem(key, value);
		} catch {
			this.failed = true;
		}
	}

	/** The selected view, `combined` when unset or unreadable. */
	readView(): ViewName {
		const raw = this.read(preferenceKey(this.projectId, 'view'));
		return isViewName(raw) ? raw : 'combined';
	}

	writeView(view: ViewName): void {
		this.write(preferenceKey(this.projectId, 'view'), view);
	}

	/**
	 * The expanded node ids for `view`, or `null` when the project has no stored
	 * expansion yet (callers then default to expanding everything).
	 */
	readExpanded(view: ViewName): string[] | null {
		return parseStringArray(this.read(preferenceKey(this.projectId, 'expanded', view)));
	}

	writeExpanded(view: ViewName, ids: string[]): void {
		this.write(preferenceKey(this.projectId, 'expanded', view), JSON.stringify(ids));
	}

	/** Dependency overlays default to hidden: they are optional. */
	readDependencies(view: ViewName): boolean {
		return this.read(preferenceKey(this.projectId, 'dependencies', view)) === 'true';
	}

	writeDependencies(view: ViewName, visible: boolean): void {
		this.write(preferenceKey(this.projectId, 'dependencies', view), String(visible));
	}

	readScroll(view: ViewName): number {
		return parseScroll(this.read(preferenceKey(this.projectId, 'scroll', view)));
	}

	writeScroll(view: ViewName, offset: number): void {
		this.write(
			preferenceKey(this.projectId, 'scroll', view),
			String(Number.isFinite(offset) && offset > 0 ? Math.round(offset) : 0)
		);
	}
}

/**
 * cyrb53: a small, deterministic 53-bit string hash.
 *
 * The identifier only needs to separate projects on one origin; it is not a
 * security boundary and deliberately reveals no path.
 */
function hash53(input: string): number {
	let h1 = 0xdeadbeef;
	let h2 = 0x41c6ce57;
	for (let index = 0; index < input.length; index += 1) {
		const code = input.charCodeAt(index);
		h1 = Math.imul(h1 ^ code, 2654435761);
		h2 = Math.imul(h2 ^ code, 1597334677);
	}
	h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507);
	h1 ^= Math.imul(h2 ^ (h2 >>> 13), 3266489909);
	h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507);
	h2 ^= Math.imul(h1 ^ (h1 >>> 13), 3266489909);
	return 4294967296 * (2097151 & h2) + (h1 >>> 0);
}
