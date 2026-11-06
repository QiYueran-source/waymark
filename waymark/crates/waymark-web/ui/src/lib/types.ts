/**
 * The JSON shapes the read-only Web API returns.
 *
 * These mirror `crates/waymark-web/src/api.rs` exactly. Nothing here is derived:
 * `state`, `ready`, `blocked`, `blocked_reason`, `shape` and `body_html` are
 * computed by the server, and the UI only presents them. In particular, custom
 * field names are opaque keys and acquire no special semantics.
 */

/** The fixed shape vocabulary; a category's `shape` indexes this list. */
export const SHAPES = [
	'circle',
	'square',
	'triangle',
	'diamond',
	'hexagon',
	'star'
] as const;

/** A stable category shape name. */
export type ShapeName = (typeof SHAPES)[number];

/** The derived, renderable work state. */
export type WorkState = 'ready' | 'in_progress' | 'blocked' | 'closed';

/** The actual stored status. */
export type WorkStatus = 'open' | 'in_progress' | 'closed';

/** A configured category with its stable shape index. */
export interface Category {
	key: string;
	name: string;
	shape: number;
}

/** One hierarchy node, as the tree endpoint returns it. */
export interface TreeNode {
	id: string;
	title: string;
	category: Category;
	status: WorkStatus;
	state: WorkState;
	ready: boolean;
	blocked: boolean;
	blocked_reason: string | null;
	parent: string | null;
	dependencies: string[];
	children: TreeNode[];
}

/** A known item whose parent relation cannot be resolved. */
export interface UnresolvedItem {
	id: string;
	title: string;
	category: Category;
	parent: string;
	reason: 'parent_missing' | 'parent_archived' | 'parent_cycle';
}

/** The combined tree payload. */
export interface CombinedTree {
	view: 'combined';
	roots: TreeNode[];
	unresolved: UnresolvedItem[];
	categories: Category[];
}

/** One category group in the grouped payload. */
export interface CategoryGroup {
	category: Category;
	roots: TreeNode[];
}

/** The category-grouped tree payload. */
export interface GroupedTree {
	view: 'grouped';
	groups: CategoryGroup[];
	unresolved: UnresolvedItem[];
	categories: Category[];
}

/** Either tree payload. */
export type TreeData = CombinedTree | GroupedTree;

/** The selected view, remembered per project in browser storage. */
export type ViewName = TreeData['view'];

/**
 * How the hierarchy is presented: as an indented tree or as a graph.
 *
 * A presentation is a rendering choice, not a data view: both presentations
 * read the same `TreeData`, and the choice is stored per project separately
 * from [`ViewName`].
 */
export type PresentationName = 'tree' | 'graph';

/** `GET /api/health` data. */
export interface Health {
	status: string;
	service: string;
	version: string;
	read_only: boolean;
	/** Present from the health endpoint; the root is canonical when possible. */
	project: { root: string };
}

/**
 * A diagnostic finding, as `waymark_core::envelope::Finding` serializes it:
 * `{severity, code, location?, message, fixable}`. `location` is absent when
 * the finding names no place.
 */
export interface Finding {
	severity: 'error' | 'warning' | 'info';
	code: string;
	location?: Record<string, unknown>;
	message: string;
	fixable: boolean;
}

/** The derived-cache entry counts by class, always summing to `total`. */
export interface CacheClassCounts {
	body: number;
	detail: number;
	query: number;
}

/** The derived-cache summary the status endpoint reports. */
export interface CacheStatus {
	/** Whether the derived-data cache accepts entries. */
	enabled: boolean;
	/** The configured entry budget. */
	capacity: number;
	/** The current total number of derived cache entries. */
	total: number;
	/** The per-class entry counts, always summing to `total`. */
	by_class: CacheClassCounts;
}

/** The lifetime hit/miss/eviction counters of the shared projection's cache. */
export interface CacheCounters {
	hits: number;
	misses: number;
	evictions: number;
}

/** The projection half of the status payload. */
export interface ProjectionStatus {
	/** How much of the project the projection covers. */
	completeness: 'complete' | 'scoped';
	/**
	 * How current the answer is. The lifecycle wins: a stale snapshot reports
	 * `stale`, a never-loaded one `unknown`.
	 */
	freshness: 'fresh' | 'stale' | 'unknown';
	/** Documents that could not be read or parsed. */
	unreadable_documents: number;
	/** Item-shaped inventory paths that could not be classified. */
	uncertain_paths: number;
	/** The findings that explain an incomplete projection. */
	diagnostics: Finding[];
	/** Whether a snapshot exists (startup or a refresh succeeded). */
	loaded: boolean;
	/** Last successful load, RFC 3339 UTC, or `null` before the first one. */
	last_success: string | null;
}

/** `GET /api/status` data: the projection and cache state, aggregates only. */
export interface StatusData {
	projection: ProjectionStatus;
	cache: CacheStatus;
	counters: CacheCounters;
}

/** Human-readable projection completeness. */
export function completenessLabel(completeness: ProjectionStatus['completeness']): string {
	return completeness === 'complete' ? 'Complete' : 'Incomplete';
}

/** Human-readable projection freshness. */
export function freshnessLabel(freshness: ProjectionStatus['freshness']): string {
	switch (freshness) {
		case 'fresh':
			return 'Fresh';
		case 'stale':
			return 'Stale';
		case 'unknown':
			return 'Unknown';
	}
}

/** One item's read-only detail. */
export interface ItemRelationships {
	parent: string | null;
	children: string[];
	dependencies: string[];
	dependents: string[];
	soft_links: string[];
}

/** Persisted instants in the viewer's effective zone. */
export interface DisplayTimes {
	/** IANA zone the values below were rendered in. */
	timezone: string;
	created_at: string;
	updated_at: string;
	closed_at: string | null;
}

/** `GET /api/item/:id` data. */
export interface ItemDetail {
	id: string;
	/** Plain text. Always rendered with text interpolation, never `{@html}`. */
	title: string;
	/** Sanitized by the server; the only field the UI injects as HTML. */
	body_html: string;
	category: Category;
	status: WorkStatus;
	state: WorkState;
	ready: boolean;
	blocked: boolean;
	blocked_reason: string | null;
	owner: string | null;
	creator: string | null;
	created_at: string | null;
	updated_at: string | null;
	closed_at: string | null;
	/**
	 * The same instants rendered in the machine's effective zone. The server
	 * resolves the zone from the local user configuration; the UI never picks one.
	 */
	display_times: DisplayTimes;
	archived: boolean;
	custom_fields: Record<string, unknown>;
	relationships: ItemRelationships;
}

/**
 * Why an unresolved item is not in the hierarchy, in the words the UI shows.
 *
 * The server names the case; the wording belongs to the app. It lives here,
 * beside the other value-to-label helpers, so the unresolved-relations list and
 * the graph's text alternative state a reason identically.
 */
export function unresolvedReason(item: UnresolvedItem): string {
	switch (item.reason) {
		case 'parent_missing':
			return `its parent ${item.parent} is missing`;
		case 'parent_archived':
			return `its parent ${item.parent} is archived`;
		case 'parent_cycle':
			return `its parent ${item.parent} forms a cycle`;
		default:
			// A reason this build does not know still needs words: rendering
			// `undefined` would leave the reader with a blank explanation.
			return 'its parent relation is unresolved';
	}
}

/** The shape index resolved against the fixed vocabulary. */
export function shapeFor(category: Category): ShapeName {
	const index = Number.isFinite(category.shape) ? Math.abs(Math.trunc(category.shape)) : 0;
	return SHAPES[index % SHAPES.length];
}

/** Human-readable actual status; derived state is shown separately. */
export function statusLabel(status: WorkStatus): string {
	switch (status) {
		case 'open':
			return 'Open';
		case 'in_progress':
			return 'In progress';
		case 'closed':
			return 'Closed';
	}
}

/** Human-readable derived state. */
export function stateLabel(state: WorkState): string {
	switch (state) {
		case 'ready':
			return 'Ready';
		case 'in_progress':
			return 'In progress';
		case 'blocked':
			return 'Blocked';
		case 'closed':
			return 'Closed';
	}
}
