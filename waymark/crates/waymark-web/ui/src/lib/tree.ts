/**
 * Pure tree helpers: visible rows, node lookups, dependency edges, and the
 * grouped view's cross-category reference nodes.
 *
 * No synthetic root is ever created. A node without a parent in the payload is
 * a root because the server says so; this module only walks `children`.
 */

import type { Category, CategoryGroup, TreeData, TreeNode } from './types';

/** One visible row: depth 0 is a root. */
export interface TreeRow {
	node: TreeNode;
	depth: number;
	hasChildren: boolean;
	expanded: boolean;
}

/** Walk the visible part of a tree, honouring the expanded set. */
export function flatten(roots: TreeNode[], expanded: ReadonlySet<string>): TreeRow[] {
	const rows: TreeRow[] = [];
	const walk = (nodes: TreeNode[], depth: number): void => {
		for (const node of nodes) {
			const hasChildren = node.children.length > 0;
			const isExpanded = hasChildren && expanded.has(node.id);
			rows.push({ node, depth, hasChildren, expanded: isExpanded });
			if (isExpanded) {
				walk(node.children, depth + 1);
			}
		}
	};
	walk(roots, 0);
	return rows;
}

/** Every node id in a tree, used to expand a whole tree by default. */
export function allIds(roots: TreeNode[]): string[] {
	const ids: string[] = [];
	const walk = (nodes: TreeNode[]): void => {
		for (const node of nodes) {
			ids.push(node.id);
			walk(node.children);
		}
	};
	walk(roots);
	return ids;
}

/** Where a node sits in the tree: its parent and, for grouped data, its group. */
export interface NodeLocation {
	node: TreeNode;
	parentId: string | null;
	group: string | null;
}

/** An id → location index over one or more root lists. */
export type NodeIndex = Map<string, NodeLocation>;

/** Index a root list, remembering parent ids. */
export function buildIndex(roots: TreeNode[], group: string | null = null): NodeIndex {
	const index: NodeIndex = new Map();
	const walk = (nodes: TreeNode[], parentId: string | null): void => {
		for (const node of nodes) {
			index.set(node.id, { node, parentId, group });
			walk(node.children, node.id);
		}
	};
	walk(roots, null);
	return index;
}

/** Index every group of the grouped view. */
export function buildGroupedIndex(groups: CategoryGroup[]): NodeIndex {
	const index: NodeIndex = new Map();
	for (const group of groups) {
		for (const [id, location] of buildIndex(group.roots, group.category.key)) {
			index.set(id, location);
		}
	}
	return index;
}

/** A dashed dependency edge belonging to one node. */
export interface DependencyEdge {
	from: string;
	fromTitle: string;
	to: string;
	toTitle: string | null;
	crossCategory: boolean;
}

/** Edges for one node, resolving target titles when the target is displayed. */
export function dependencyEdges(node: TreeNode, index: NodeIndex): DependencyEdge[] {
	return node.dependencies.map((to) => {
		const target = index.get(to);
		return {
			from: node.id,
			fromTitle: node.title,
			to,
			toTitle: target?.node.title ?? null,
			crossCategory: target !== undefined && target.node.category.key !== node.category.key
		};
	});
}

/**
 * A reference node for a cross-category dependency in the grouped view.
 *
 * The displayed item `forId` depends on `toId` in another category. The
 * reference is anchored to the referenced item's parent when it has one, and to
 * the referenced item itself when it is a root, so the connection point is
 * always a real row of this view. Hiding dependency overlays hides these nodes
 * too.
 */
export interface ReferenceNode {
	anchor: string;
	/** True when the anchor is the referenced item itself (it has no parent). */
	selfAnchored: boolean;
	forId: string;
	forTitle: string;
	toId: string;
	toTitle: string;
}

/** Every cross-category reference in a grouped tree. */
export function groupedReferences(groups: CategoryGroup[]): ReferenceNode[] {
	const index = buildGroupedIndex(groups);
	const references: ReferenceNode[] = [];
	for (const location of index.values()) {
		const dependent = location.node;
		for (const toId of dependent.dependencies) {
			const target = index.get(toId);
			if (target === undefined || target.node.category.key === dependent.category.key) {
				continue;
			}
			const anchor = target.parentId ?? target.node.id;
			references.push({
				anchor,
				selfAnchored: target.parentId === null,
				forId: dependent.id,
				forTitle: dependent.title,
				toId,
				toTitle: target.node.title
			});
		}
	}
	return references;
}

/** Index reference nodes by their anchor (a node id or a group root marker). */
export function referencesByAnchor(references: ReferenceNode[]): Map<string, ReferenceNode[]> {
	const byAnchor = new Map<string, ReferenceNode[]>();
	for (const reference of references) {
		const bucket = byAnchor.get(reference.anchor);
		if (bucket) bucket.push(reference);
		else byAnchor.set(reference.anchor, [reference]);
	}
	return byAnchor;
}

/**
 * A top-level section of a tree view.
 *
 * The combined view is its own kind of section, not a category: it has no key
 * and no category at all, so nothing here can collide with a real category key.
 */
export type TreeSection =
	| { kind: 'combined'; roots: TreeNode[] }
	| { kind: 'category'; category: Category; roots: TreeNode[] };

/** The displayed sections of a tree payload, independent of view. */
export function sectionsFor(data: TreeData): TreeSection[] {
	if (data.view === 'combined') {
		return [{ kind: 'combined', roots: data.roots }];
	}
	return data.groups.map((group) => ({ kind: 'category', category: group.category, roots: group.roots }));
}

/** Every displayed root, in section order. */
export function visibleRoots(data: TreeData): TreeNode[] {
	return sectionsFor(data).flatMap((section) => section.roots);
}

/**
 * The categories a reader has in scope, out of every configured category.
 *
 * `configured` is the payload's `categories` — every category the project
 * configures, in configured order, whether or not it holds active work. `stored`
 * is what browser storage remembered, or `null` when this project has no stored
 * selection yet (the reader then sees everything). Stored keys for categories
 * this project no longer configures are dropped; a stored empty list means
 * "nothing selected", which is a real choice and stays empty.
 */
export function resolveCategoryScope(
	configured: Category[],
	stored: string[] | null
): Set<string> {
	if (stored === null) return new Set(configured.map((category) => category.key));
	const known = new Set(configured.map((category) => category.key));
	return new Set(stored.filter((key) => known.has(key)));
}

/** A scope as it is remembered: configured order, no stale or duplicated keys. */
export function categoryScopeKeys(configured: Category[], selected: ReadonlySet<string>): string[] {
	return configured
		.map((category) => category.key)
		.filter((key) => selected.has(key));
}

/** How much work is displayed, and how much of it is blocked. */
export interface TreeCounts {
	items: number;
	blocked: number;
}

/** Count every displayed item, including nested children. */
export function countNodes(nodes: TreeNode[]): TreeCounts {
	let items = 0;
	let blocked = 0;
	const walk = (list: TreeNode[]): void => {
		for (const node of list) {
			items += 1;
			if (node.state === 'blocked') blocked += 1;
			walk(node.children);
		}
	};
	walk(nodes);
	return { items, blocked };
}

/** The outcome of filtering the grouped view by category scope. */
export interface FilteredGroups {
	/** The surviving groups, in payload order, with their filtered roots. */
	groups: CategoryGroup[];
	/** Counts of the work items in scope, not of anything shown as context. */
	counts: TreeCounts;
	/**
	 * Ids of rows kept only to preserve parentage: their category is outside the
	 * selection, and a selected work item sits below them. They are
	 * presentation-only and never selected work, so they are left out of
	 * `counts` — but they do keep their group on screen, because the selected
	 * item below them has to stay under its real parent.
	 */
	contextIds: ReadonlySet<string>;
}

/**
 * Keep the work items whose category is in `selected`, and any ancestor needed
 * to keep them under their real parents.
 *
 * The rule, level by level:
 *
 * - an in-scope node is always kept, whether or not it has kept children;
 * - an out-of-scope node is kept only when its filtered subtree holds at least
 *   one kept node, so the chain to selected work survives and its unrelated
 *   out-of-scope siblings do not;
 * - a kept out-of-scope node is context: its id goes in `contextIds` and it is
 *   left out of `counts`, while selected descendants below it are counted.
 *
 * Nodes are never re-parented or promoted: a selected child stays under its real
 * parent, and when several levels are out of scope the whole chain is kept.
 * Groups are never re-homed either: a context ancestor keeps the group its real
 * root belongs to, and a group with nothing left drops out. That is why an
 * unselected category can still appear — as a context-only group — while a
 * selected category with no active work still shows nothing.
 */
export function filterGroups(
	groups: CategoryGroup[],
	selected: ReadonlySet<string>
): FilteredGroups {
	const kept: CategoryGroup[] = [];
	const counts: TreeCounts = { items: 0, blocked: 0 };
	const contextIds = new Set<string>();
	for (const group of groups) {
		const filtered = filterNodes(group.roots, selected);
		if (filtered.nodes.length === 0) continue;
		kept.push({ category: group.category, roots: filtered.nodes });
		counts.items += filtered.counts.items;
		counts.blocked += filtered.counts.blocked;
		for (const id of filtered.contextIds) contextIds.add(id);
	}
	return { groups: kept, counts, contextIds };
}

interface FilteredNodes {
	nodes: TreeNode[];
	counts: TreeCounts;
	contextIds: Set<string>;
}

function filterNodes(nodes: TreeNode[], selected: ReadonlySet<string>): FilteredNodes {
	const kept: TreeNode[] = [];
	const counts: TreeCounts = { items: 0, blocked: 0 };
	const contextIds = new Set<string>();
	for (const node of nodes) {
		const inScope = selected.has(node.category.key);
		const children = filterNodes(node.children, selected);
		// An out-of-scope node survives only as the chain to kept work. Keeping
		// an in-scope node is unconditional: it is selected work in its own
		// right, and a leaf is not hidden just because it has no kept children.
		if (!inScope && children.nodes.length === 0) continue;
		kept.push({ ...node, children: children.nodes });
		if (inScope) {
			counts.items += 1 + children.counts.items;
			counts.blocked += (node.state === 'blocked' ? 1 : 0) + children.counts.blocked;
		} else {
			contextIds.add(node.id);
			counts.items += children.counts.items;
			counts.blocked += children.counts.blocked;
		}
		for (const id of children.contextIds) contextIds.add(id);
	}
	return { nodes: kept, counts, contextIds };
}

/** The summary wording for a set of counts, so a filtered view reads the same. */
export function summarizeCounts(counts: TreeCounts, unresolved: number): string {
	const parts = [
		`${counts.items} item${counts.items === 1 ? '' : 's'}`,
		`${counts.blocked} blocked`
	];
	if (unresolved > 0) parts.push(`${unresolved} unresolved`);
	return parts.join(' · ');
}

/** The screen's one finding: how much work there is and what is held up. */
export function summarize(data: TreeData): string {
	return summarizeCounts(countNodes(visibleRoots(data)), data.unresolved.length);
}
