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

/** What the payload asks the drawing to include beyond the hierarchy. */
export interface BuildGraphOptions {
	/** Draw the dashed dependency overlay. Off unless asked for. */
	dependenciesVisible?: boolean;
}

function toGraphNode(node: TreeNode, parentId: string | null, depth: number): GraphNode {
	return {
		id: node.id,
		title: node.title,
		category: node.category,
		shape: shapeFor(node.category),
		status: node.status,
		state: node.state,
		blocked: node.blocked,
		blockedReason: node.blocked_reason,
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

	const walk = (tree: TreeNode, parentId: string | null, depth: number): void => {
		if (seen.has(tree.id)) return;
		seen.add(tree.id);
		nodes.push(toGraphNode(tree, parentId, depth));
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

/** The accessible name of one graph node: title, category, and state. */
export function graphNodeLabel(node: GraphNode): string {
	return `${node.title} — ${node.category.name}, ${stateLabel(node.state)}`;
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
