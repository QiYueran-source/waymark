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

import { MAX_GRAPH_ZOOM, MIN_GRAPH_ZOOM, type GraphViewport } from './graph';
import type { PresentationName, ViewName } from './types';

/** The preference namespace. Bump the version if the stored shape changes. */
export const STORAGE_PREFIX = 'waymark.web.v1';

/**
 * Collapse path spelling differences that do not identify a different root.
 *
 * The health read canonicalizes the root, but its fallback is the opened path,
 * which may still carry `.`, `..`, or a trailing separator. Folding those here
 * keeps `/a/b`, `/a/b/`, and `/a/./b` in one namespace even then: a project
 * that moved must not silently inherit a namespace it no longer has, and a
 * project reached through a different spelling must not lose its own.
 */
export function normalizeProjectRoot(root: string): string {
	const value = root.trim().replace(/\\/g, '/').replace(/\/{2,}/g, '/');
	const absolute = value.startsWith('/');
	const segments: string[] = [];
	for (const segment of value.split('/')) {
		if (segment === '' || segment === '.') continue;
		if (segment === '..') {
			if (segments.length > 0 && segments[segments.length - 1] !== '..') {
				segments.pop();
			} else if (!absolute) {
				// A relative root cannot climb above its own start; keep the
				// segment so `../a` stays distinct from `a`.
				segments.push(segment);
			}
			continue;
		}
		segments.push(segment);
	}
	const folded = segments.join('/');
	if (folded === '') return '/';
	return absolute ? `/${folded}` : folded;
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
	preference:
		| 'view'
		| 'presentation'
		| 'expanded'
		| 'dependencies'
		| 'scroll'
		| 'categories'
		| 'collapsed'
		| 'viewport'
		| 'graphAlternative',
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

function isPresentationName(value: string | null): value is PresentationName {
	return value === 'tree' || value === 'graph';
}

/**
 * A stored list of strings, or `null` when there is nothing usable.
 *
 * Entries of the wrong type are dropped, so one corrupted element cannot
 * discard a readable selection. An array with no usable entry at all is not a
 * choice the reader made — `[]` means "none", and defaulting is the only safe
 * reading of `[7, null]` — so it reads as missing rather than as "none".
 */
function parseStringArray(raw: string | null): string[] | null {
	if (raw === null) return null;
	try {
		const parsed: unknown = JSON.parse(raw);
		if (!Array.isArray(parsed)) return null;
		const entries = parsed.filter((entry): entry is string => typeof entry === 'string');
		return parsed.length > 0 && entries.length === 0 ? null : entries;
	} catch {
		return null;
	}
}

function parseScroll(raw: string | null): number {
	if (raw === null) return 0;
	const value = Number(raw);
	return Number.isFinite(value) && value >= 0 ? value : 0;
}

/**
 * A stored viewport, or `null` when there is nothing usable to restore.
 *
 * Every value is checked: a record written by an older version, hand-edited, or
 * truncated must not put the drawing somewhere it cannot be seen from.
 */
function parseViewport(raw: string | null): GraphViewport | null {
	if (raw === null) return null;
	try {
		const parsed: unknown = JSON.parse(raw);
		if (typeof parsed !== 'object' || parsed === null) return null;
		const { x, y, zoom } = parsed as Record<string, unknown>;
		if (typeof x !== 'number' || !Number.isFinite(x)) return null;
		if (typeof y !== 'number' || !Number.isFinite(y)) return null;
		if (typeof zoom !== 'number' || !Number.isFinite(zoom)) return null;
		if (zoom < MIN_GRAPH_ZOOM || zoom > MAX_GRAPH_ZOOM) return null;
		return { x, y, zoom };
	} catch {
		return null;
	}
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

	/** Remove one key; unavailable or failing storage only sets `failed`. */
	private clear(key: string): void {
		if (this.storage === null) {
			this.failed = true;
			return;
		}
		try {
			this.storage.removeItem(key);
		} catch {
			this.failed = true;
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
	 * The chosen presentation, `tree` when unset or unreadable.
	 *
	 * One presentation per project, not per view: the graph and the tree both
	 * render whichever view is selected, so switching between `combined` and
	 * `grouped` must not silently change how the reader is looking at it.
	 */
	readPresentation(): PresentationName {
		const raw = this.read(preferenceKey(this.projectId, 'presentation'));
		return isPresentationName(raw) ? raw : 'tree';
	}

	writePresentation(presentation: PresentationName): void {
		this.write(preferenceKey(this.projectId, 'presentation'), presentation);
	}

	/**
	 * Whether the graph is being read as its structured text alternative.
	 *
	 * It is a property of the project, not of one data view, and it is on its
	 * own key so the graph's collapse set and its viewport stay independent of
	 * it. The drawing is the default: the alternative is an explicit choice.
	 */
	readGraphAlternative(): boolean {
		return this.read(preferenceKey(this.projectId, 'graphAlternative')) === 'true';
	}

	writeGraphAlternative(alternative: boolean): void {
		this.write(preferenceKey(this.projectId, 'graphAlternative'), String(alternative));
	}

	/**
	 * The category keys the reader has in scope, or `null` when this project has
	 * no stored selection (`null` means "show every configured category"; an
	 * empty list means "the reader cleared them all").
	 *
	 * The scope is one per project, not per view: the grouped tree and the graph
	 * filter by the same selection. Keys this project no longer configures are
	 * kept here and dropped where the scope meets the project's categories.
	 */
	readCategories(): string[] | null {
		return parseStringArray(this.read(preferenceKey(this.projectId, 'categories')));
	}

	writeCategories(keys: string[]): void {
		this.write(preferenceKey(this.projectId, 'categories'), JSON.stringify(keys));
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

	/**
	 * The graph nodes the reader has collapsed, or `null` when this project has
	 * none stored.
	 *
	 * One set per project, deliberately not per view: the graph draws whichever
	 * view is selected, so collapsing a busy branch and then switching between
	 * `combined` and `grouped` must not silently reopen it. Ids that the drawing
	 * no longer contains are kept here and ignored where the set meets the
	 * current data, which is what makes a stored set survive project changes.
	 */
	readCollapsed(): string[] | null {
		return parseStringArray(this.read(preferenceKey(this.projectId, 'collapsed')));
	}

	writeCollapsed(ids: string[]): void {
		this.write(preferenceKey(this.projectId, 'collapsed'), JSON.stringify(ids));
	}

	/**
	 * The graph viewport the reader last left, or `null` when there is none.
	 *
	 * It is stored on its own key — never as the tree pane's scroll offset — so
	 * the graph's pan and zoom survive a reload without moving the tree.
	 */
	readViewport(): GraphViewport | null {
		return parseViewport(this.read(preferenceKey(this.projectId, 'viewport')));
	}

	writeViewport(viewport: GraphViewport): void {
		this.write(
			preferenceKey(this.projectId, 'viewport'),
			JSON.stringify({ x: viewport.x, y: viewport.y, zoom: viewport.zoom })
		);
	}

	/** Forget the stored viewport, so the next entry fits the drawing instead. */
	clearViewport(): void {
		this.clear(preferenceKey(this.projectId, 'viewport'));
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
