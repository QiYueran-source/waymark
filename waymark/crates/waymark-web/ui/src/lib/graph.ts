/**
 * The pure graph model behind the graph presentation.
 *
 * It walks the same tree payload the tree view renders and produces the nodes,
 * the solid parent-child edges, the dashed dependency edges, and the roots of
 * the hierarchy. It imports no rendering library and no Svelte component, so it
 * can be unit-tested directly and so every graph ticket shares one description
 * of the hierarchy.
 *
 * Nothing here is derived from the server's data except presentation: the
 * category shape, the work state, and the blocked marker all come from the
 * payload. Dependency edges are an optional overlay — they exist only while the
 * reader has dependency visibility on, and they never change the hierarchy
 * drawing or its layout.
 */

import { sectionsFor } from './tree';
import {
	shapeFor,
	stateLabel,
	statusLabel,
	type Category,
	type ShapeName,
	type TreeData,
	type TreeNode,
	type UnresolvedItem,
	type WorkState,
	type WorkStatus
} from './types';

/** One node of the graph: a displayed work item and its place in the tree. */
export interface GraphNode {
	id: string;
	title: string;
	category: Category;
	shape: ShapeName;
	status: WorkStatus;
	state: WorkState;
	blocked: boolean;
	blockedReason: string | null;
	/**
	 * Kept only to preserve parentage under the current category scope: the
	 * item's own category is outside the selection, and selected work sits below
	 * it. It is not selected work, so the card and its accessible name say so —
	 * with the same wording the tree uses.
	 */
	context: boolean;
	/** The displayed parent, or null when this node is a root of its section. */
	parentId: string | null;
	/** 0 for a root; one more than the parent's depth below it. */
	depth: number;
	hasChildren: boolean;
	/**
	 * The ids this item depends on, as the payload states them. They are not
	 * resolved here: a target that is not drawn yields no edge at all.
	 */
	dependencies: string[];
}

/** A solid parent-child edge. */
export interface GraphEdge {
	id: string;
	kind: 'parent-child';
	from: string;
	to: string;
}

/**
 * A dashed dependency edge, drawn when dependency visibility is on.
 *
 * `from` depends on `to`, so the arrowhead on the edge points at `to`: the
 * depended-on item. Both ends are always drawn — a dependency whose target is
 * not in the drawing yields no edge, not a dangling one.
 */
export interface GraphDependencyEdge {
	id: string;
	kind: 'dependency';
	from: string;
	to: string;
	/** The depended-on item's title; it is drawn, so it is always known. */
	toTitle: string;
	/** True when the two ends belong to different categories. */
	crossCategory: boolean;
}

/** Any edge the graph can draw. */
export type AnyGraphEdge = GraphEdge | GraphDependencyEdge;

/** Everything one rendering of the hierarchy needs. */
export interface WorkGraph {
	nodes: GraphNode[];
	/** Solid parent-child edges: the hierarchy, and the only layout input. */
	edges: GraphEdge[];
	/** Dashed dependency edges; empty while dependency visibility is off. */
	dependencies: GraphDependencyEdge[];
	/** Root node ids, in payload order. Never a synthetic node. */
	roots: string[];
	/** Items whose parent relation cannot be resolved; the graph omits them. */
	unresolved: UnresolvedItem[];
}

const EMPTY_CONTEXT: ReadonlySet<string> = new Set();

/** What the payload asks the drawing to include beyond the hierarchy. */
export interface BuildGraphOptions {
	/** Draw the dashed dependency overlay. Off unless asked for. */
	dependenciesVisible?: boolean;
	/**
	 * Ids the filtered payload kept only to preserve parentage. They are the
	 * by-category tree's `contextIds`, handed over unchanged so the two
	 * presentations cannot disagree about what is context. Defaults to none,
	 * which is what an unfiltered payload needs.
	 */
	contextIds?: ReadonlySet<string>;
}

function toGraphNode(
	node: TreeNode,
	parentId: string | null,
	depth: number,
	contextIds: ReadonlySet<string>
): GraphNode {
	return {
		id: node.id,
		title: node.title,
		category: node.category,
		shape: shapeFor(node.category),
		status: node.status,
		state: node.state,
		blocked: node.blocked,
		blockedReason: node.blocked_reason,
		context: contextIds.has(node.id),
		parentId,
		depth,
		hasChildren: node.children.length > 0,
		dependencies: [...node.dependencies]
	};
}

/**
 * Build the graph for a tree payload.
 *
 * The walk restarts at every section root, so a node is a root of the graph
 * when the payload shows it as a root of its section — the graph never invents
 * a parent, and never invents a root either. An item reached twice (a payload
 * duplication) is drawn once.
 *
 * Dependency edges are resolved against the drawn set, in node order and then
 * in the payload's dependency order. A dependency on an item this drawing does
 * not contain — archived, unresolved, filtered out — is dropped silently: the
 * graph neither draws an edge into nothing nor invents a placeholder. They are
 * kept apart from `edges` because they are an overlay: the hierarchy layout
 * must not move to make room for them.
 */
export function buildGraph(data: TreeData, options: BuildGraphOptions = {}): WorkGraph {
	const nodes: GraphNode[] = [];
	const edges: GraphEdge[] = [];
	const roots: string[] = [];
	const seen = new Set<string>();
	const contextIds = options.contextIds ?? EMPTY_CONTEXT;

	const walk = (tree: TreeNode, parentId: string | null, depth: number): void => {
		if (seen.has(tree.id)) return;
		seen.add(tree.id);
		nodes.push(toGraphNode(tree, parentId, depth, contextIds));
		if (parentId !== null) {
			edges.push({
				id: `${parentId}->${tree.id}`,
				kind: 'parent-child',
				from: parentId,
				to: tree.id
			});
		}
		for (const child of tree.children) {
			walk(child, tree.id, depth + 1);
		}
	};

	for (const section of sectionsFor(data)) {
		for (const root of section.roots) {
			if (seen.has(root.id)) continue;
			roots.push(root.id);
			walk(root, null, 0);
		}
	}

	return {
		nodes,
		edges,
		dependencies: options.dependenciesVisible ? dependencyEdges(nodes) : [],
		roots,
		unresolved: data.unresolved
	};
}

/** One dashed edge per dependency of a drawn item whose target is drawn too. */
function dependencyEdges(nodes: GraphNode[]): GraphDependencyEdge[] {
	const drawn = new Map(nodes.map((node) => [node.id, node]));
	const edges: GraphDependencyEdge[] = [];
	for (const node of nodes) {
		for (const targetId of node.dependencies) {
			const target = drawn.get(targetId);
			if (target === undefined) continue;
			edges.push({
				id: `dependency:${node.id}->${targetId}`,
				kind: 'dependency',
				from: node.id,
				to: targetId,
				toTitle: target.title,
				crossCategory: target.category.key !== node.category.key
			});
		}
	}
	return edges;
}

/**
 * What the node adapter is handed for one drawn node: the model values plus
 * where the layout put it and whether it is the selected item.
 *
 * The rendering library passes a node's `data` straight to the node component,
 * so this is the whole contract between the drawing and the card. It keeps
 * `GraphNode.svelte` free of library props.
 */
export interface GraphNodePayload extends Record<string, unknown> {
	node: GraphNode;
	/** Top edge of the node's place in the drawing: one row per generation. */
	graphY: number;
	selected: boolean;
	onSelect: (id: string, trigger?: HTMLElement) => void;
}

/**
 * The wording that marks an item kept only for its place in a filtered
 * hierarchy. It is the tree's phrase, word for word, so a reader who switches
 * presentation hears the same thing about the same item.
 */
export const CONTEXT_NOTE = 'context, outside the selection';

/** The accessible name of one graph node: title, category, state, and context. */
export function graphNodeLabel(node: GraphNode): string {
	const name = `${node.title} — ${node.category.name}, ${stateLabel(node.state)}`;
	return node.context ? `${name} (${CONTEXT_NOTE})` : name;
}

/**
 * The accessible name of one edge: it states the relation and both titles, and
 * for a dependency it says which end depends on which, so the direction is
 * readable without seeing the arrowhead.
 */
export function edgeLabel(graph: WorkGraph, edge: AnyGraphEdge): string {
	const from = graph.nodes.find((node) => node.id === edge.from);
	const to = graph.nodes.find((node) => node.id === edge.to);
	if (edge.kind === 'dependency') {
		return `Dependency: ${from?.title ?? edge.from} → ${edge.toTitle} (depends on)`;
	}
	return `Parent-child: ${from?.title ?? edge.from} → ${to?.title ?? edge.to}`;
}

/**
 * One entry of the graph's structured text alternative.
 *
 * A drawn item carries its place in the hierarchy and every relation it takes
 * part in, each one as a sentence: a reader who never sees an edge still gets
 * the parent and the children, and — while dependency visibility is on — which
 * item depends on which. An unresolved item carries the reason it cannot be
 * placed, so the alternative accounts for everything the graph knows about.
 *
 * The values are already words, not model values: the component that renders
 * this only has to put them in a list. That keeps the plain-text reading in
 * one testable place and off colour and shape.
 */
export interface OutlineItemEntry {
	kind: 'item';
	id: string;
	title: string;
	/** The category name, written out. */
	category: string;
	state: string;
	status: string;
	parentId: string | null;
	depth: number;
	/** "Parent: …" — stated even for a root, so no relation is implied. */
	parent: string;
	/** "Child: …" for each child, so parentage never rests on indentation. */
	children: string[];
	/** "Depends on …" for each edge, only while dependency visibility is on. */
	dependencies: string[];
	/** "… depends on this item." for each edge that points here. */
	dependents: string[];
	blockedReason: string | null;
	/**
	 * True when the item is only kept to preserve parentage under the current
	 * category scope. The outline says so in words, exactly as the card and the
	 * tree row do, so the filter's context rule survives without a badge.
	 */
	context: boolean;
	/** "Context: …" when `context` is true, otherwise null. */
	contextNote: string | null;
}

/** One unresolved item, with the reason its parent cannot be reached. */
export interface OutlineUnresolvedEntry {
	kind: 'unresolved';
	id: string;
	title: string;
	category: string;
	reason: string;
}

export type OutlineEntry = OutlineItemEntry | OutlineUnresolvedEntry;

/**
 * The reason an unresolved item is not in the hierarchy, in the same words the
 * unresolved-relations area uses. The server names the case; the wording is the
 * app's, and it lives here so both lists state it identically.
 */
export function unresolvedReason(item: UnresolvedItem): string {
	switch (item.reason) {
		case 'parent_missing':
			return `its parent ${item.parent} is missing`;
		case 'parent_archived':
			return `its parent ${item.parent} is archived`;
		case 'parent_cycle':
			return `its parent ${item.parent} forms a cycle`;
	}
}

/**
 * The structured text alternative to the graph, as a flat, ordered outline.
 *
 * It walks `graph.nodes` — the *drawn* items, so a filtered or collapsed graph
 * yields a correspondingly shorter outline — and appends `graph.unresolved`
 * last. Dependency sentences come from `graph.dependencies`, which is empty
 * while the overlay is off, so the alternative cannot silently show a relation
 * the graph is not showing, and vice versa: both read one model.
 */
export function graphOutline(graph: WorkGraph): OutlineEntry[] {
	const byId = new Map(graph.nodes.map((node) => [node.id, node]));

	const childrenOf = new Map<string, GraphNode[]>();
	for (const node of graph.nodes) {
		if (node.parentId === null) continue;
		const siblings = childrenOf.get(node.parentId);
		if (siblings === undefined) childrenOf.set(node.parentId, [node]);
		else siblings.push(node);
	}

	const dependentsOf = new Map<string, GraphDependencyEdge[]>();
	for (const edge of graph.dependencies) {
		const list = dependentsOf.get(edge.to);
		if (list === undefined) dependentsOf.set(edge.to, [edge]);
		else list.push(edge);
	}

	const entries: OutlineEntry[] = graph.nodes.map((node) => {
		const parent = node.parentId === null ? null : (byId.get(node.parentId) ?? null);
		return {
			kind: 'item',
			id: node.id,
			title: node.title,
			category: node.category.name,
			state: stateLabel(node.state),
			status: statusLabel(node.status),
			parentId: node.parentId,
			depth: node.depth,
			parent:
				parent === null
					? 'Parent: none; this is a top-level item.'
					: `Parent: ${parent.title} (${parent.id}).`,
			children: (childrenOf.get(node.id) ?? []).map(
				(child) => `Child: ${child.title} (${child.id}).`
			),
			dependencies: graph.dependencies
				.filter((edge) => edge.from === node.id)
				.map((edge) => `Depends on ${edge.toTitle} (${edge.to}).`),
			dependents: (dependentsOf.get(node.id) ?? []).map((edge) => {
				const dependent = byId.get(edge.from);
				return `${dependent?.title ?? edge.from} (${edge.from}) depends on this item.`;
			}),
			blockedReason: node.blocked ? node.blockedReason : null,
			context: node.context,
			contextNote: node.context
				? `Context: outside the category selection (${CONTEXT_NOTE}).`
				: null
		};
	});

	for (const item of graph.unresolved) {
		entries.push({
			kind: 'unresolved',
			id: item.id,
			title: item.title,
			category: item.category.name,
			reason: unresolvedReason(item)
		});
	}

	return entries;
}
