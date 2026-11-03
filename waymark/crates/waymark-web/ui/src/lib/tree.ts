/**
 * Pure tree helpers: visible rows, node lookups, dependency edges, and the
 * grouped view's cross-category reference nodes.
 *
 * No synthetic root is ever created. A node without a parent in the payload is
 * a root because the server says so; this module only walks `children`.
 */

import type { CategoryGroup, TreeNode, ViewName } from './types';

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

/** The visible root list for a tree payload, independent of view. */
export function rootsFor(
	data: { view: 'combined'; roots: TreeNode[] } | { view: 'grouped'; groups: CategoryGroup[] },
	view: ViewName
): CategoryGroup[] {
	if (view === 'grouped' && 'groups' in data) return data.groups;
	if ('roots' in data) return [{ category: { key: '*', name: '', shape: 0 }, roots: data.roots }];
	return [];
}
