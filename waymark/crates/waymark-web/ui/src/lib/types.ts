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

/** `GET /api/health` data. */
export interface Health {
	status: string;
	service: string;
	version: string;
	read_only: boolean;
	/** Present from the health endpoint; the root is canonical when possible. */
	project: { root: string };
}

/** One item's read-only detail. */
export interface ItemRelationships {
	parent: string | null;
	children: string[];
	dependencies: string[];
	dependents: string[];
	soft_links: string[];
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
	archived: boolean;
	custom_fields: Record<string, unknown>;
	relationships: ItemRelationships;
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
