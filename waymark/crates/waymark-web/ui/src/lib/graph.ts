/**
 * The pure graph model behind the graph presentation.
 *
 * It walks the same tree payload the tree view renders and produces the nodes,
 * the solid parent-child edges, and the roots of the hierarchy. It imports no
 * rendering library and no Svelte component, so it can be unit-tested directly
 * and so every graph ticket shares one description of the hierarchy.
 *
 * Nothing here is derived from the server's data except presentation: the
 * category shape, the work state, and the blocked marker all come from the
 * payload. Dependency edges and the unresolved list are deliberately not part
 * of this drawing — the graph shows the hierarchy, nothing else.
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
}

/** A solid parent-child edge. There is no other kind of edge in this slice. */
export interface GraphEdge {
	id: string;
	from: string;
	to: string;
}

/** Everything one rendering of the hierarchy needs. */
export interface WorkGraph {
	nodes: GraphNode[];
	edges: GraphEdge[];
	/** Root node ids, in payload order. Never a synthetic node. */
	roots: string[];
	/** Items whose parent relation cannot be resolved; the graph omits them. */
	unresolved: UnresolvedItem[];
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
		hasChildren: node.children.length > 0
	};
}

/**
 * Build the graph for a tree payload.
 *
 * The walk restarts at every section root, so a node is a root of the graph
 * when the payload shows it as a root of its section — the graph never invents
 * a parent, and never invents a root either. An item reached twice (a payload
 * duplication) is drawn once.
 */
export function buildGraph(data: TreeData): WorkGraph {
	const nodes: GraphNode[] = [];
	const edges: GraphEdge[] = [];
	const roots: string[] = [];
	const seen = new Set<string>();

	const walk = (tree: TreeNode, parentId: string | null, depth: number): void => {
		if (seen.has(tree.id)) return;
		seen.add(tree.id);
		nodes.push(toGraphNode(tree, parentId, depth));
		if (parentId !== null) {
			edges.push({ id: `${parentId}->${tree.id}`, from: parentId, to: tree.id });
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

	return { nodes, edges, roots, unresolved: data.unresolved };
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

/** The accessible name of one edge: it states the relation and both titles. */
export function edgeLabel(graph: WorkGraph, edge: GraphEdge): string {
	const from = graph.nodes.find((node) => node.id === edge.from);
	const to = graph.nodes.find((node) => node.id === edge.to);
	return `Parent-child: ${from?.title ?? edge.from} → ${to?.title ?? edge.to}`;
}
