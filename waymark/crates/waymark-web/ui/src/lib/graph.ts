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
	/**
	 * True when the reader has collapsed this node, so its children are not
	 * drawn. Set by [`collapseGraph`]; a complete graph has none collapsed.
	 */
	collapsed?: boolean;
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

/**
 * A viewport: where the canvas is scrolled to and how far it is zoomed.
 *
 * The values are the drawing library's own transform — `x`/`y` in screen
 * pixels, `zoom` a scale factor — so a remembered viewport can be restored
 * exactly, and so one project's viewport cannot be mistaken for another's.
 */
export interface GraphViewport {
	x: number;
	y: number;
	zoom: number;
}

/** The zoom range a fitted viewport stays inside. */
export const MIN_GRAPH_ZOOM = 0.5;
export const MAX_GRAPH_ZOOM = 2;

/** The share of the canvas the fitted drawing leaves free at its edges. */
export const GRAPH_FIT_PADDING = 0.1;

/**
 * The canvas size assumed until the page has measured the drawing's box.
 *
 * A browser measures the box and fits against its real size. Server rendering
 * and a DOM without layout (tests) report a zero-sized box, so fitting needs a
 * nominal one: without it the first fitted viewport would be the neutral
 * `zoom: 1`, which is not a fit at all.
 */
export const GRAPH_NOMINAL_CANVAS = { width: 960, height: 640 };

/**
 * The pan/zoom operations the canvas controls need, as the app sees them.
 *
 * The drawing library satisfies this; naming it here keeps the view free of
 * the library's own types, so the controls talk to a small, replaceable seam.
 */
export interface GraphFlow {
	setViewport(viewport: GraphViewport): Promise<boolean> | boolean | void;
	/** The canvas's current transform, as the drawing holds it. */
	getViewport(): GraphViewport;
	zoomIn(): Promise<boolean> | boolean | void;
	zoomOut(): Promise<boolean> | boolean | void;
}

/** An axis-aligned box in the drawing's coordinate space. */
export interface GraphBounds {
	x: number;
	y: number;
	width: number;
	height: number;
}

/**
 * What the payload asks the drawing to include beyond the hierarchy. */
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
	/** True when this node's children are hidden by the reader's collapse. */
	collapsed: boolean;
	onSelect: (id: string, trigger?: HTMLElement) => void;
	/** Collapse or expand this node's children. Never changes the item. */
	onToggle: (id: string) => void;
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

/**
 * The drawing with the reader's collapsed nodes applied.
 *
 * Collapsing is presentation only: the payload is untouched, and the returned
 * model is a new value, so the caller's graph keeps its complete hierarchy.
 * A node that is itself collapsed stays; every descendant below it is left
 * out, along with the edges that would point at it. Ids in `collapsed` that
 * this drawing does not contain are ignored, which is what makes a stored
 * collapse set survive project changes.
 *
 * `collapsed` is marked on the nodes that remain, so a card can offer "expand"
 * without the caller passing the set down as well.
 */
export function collapseGraph(graph: WorkGraph, collapsed: ReadonlySet<string>): WorkGraph {
	if (collapsed.size === 0) return graph;

	// `graph.nodes` lists a parent before its children, so one pass is enough:
	// a node below a hidden or collapsed parent is hidden in turn.
	const hidden = new Set<string>();
	const kept: GraphNode[] = [];
	for (const node of graph.nodes) {
		if (node.parentId !== null && hidden.has(node.parentId)) {
			hidden.add(node.id);
			continue;
		}
		if (collapsed.has(node.id)) {
			hidden.add(node.id);
			kept.push({ ...node, collapsed: true });
			continue;
		}
		kept.push(node);
	}

	const drawn = new Set(kept.map((node) => node.id));
	const keepBoth = (edge: { from: string; to: string }): boolean =>
		drawn.has(edge.from) && drawn.has(edge.to);

	return {
		nodes: kept,
		edges: graph.edges.filter(keepBoth),
		dependencies: graph.dependencies.filter(keepBoth),
		roots: graph.roots.filter((id) => drawn.has(id)),
		unresolved: graph.unresolved
	};
}

/** The bounding box of the drawn nodes, or `null` when nothing is drawn. */
export function graphBounds(
	rects: ReadonlyArray<{ x: number; y: number; width: number; height: number }>
): GraphBounds | null {
	if (rects.length === 0) return null;
	let left = Infinity;
	let top = Infinity;
	let right = -Infinity;
	let bottom = -Infinity;
	for (const rect of rects) {
		left = Math.min(left, rect.x);
		top = Math.min(top, rect.y);
		right = Math.max(right, rect.x + rect.width);
		bottom = Math.max(bottom, rect.y + rect.height);
	}
	return { x: left, y: top, width: right - left, height: bottom - top };
}

/**
 * The viewport that fits `bounds` inside a canvas of `width` by `height`.
 *
 * This is the drawing's initial state and the target of "fit to view": the
 * whole drawing, centred, inside the zoom range, with a margin at the edges.
 * It is computed from the model's geometry rather than measured from the DOM,
 * so the same drawing always fits the same way and the viewport can be
 * restored exactly. An empty drawing — or a canvas that has not been measured
 * yet — yields the neutral viewport rather than an infinite zoom.
 */
export function fittedViewport(
	bounds: GraphBounds | null,
	width: number,
	height: number
): GraphViewport {
	if (bounds === null || bounds.width <= 0 || bounds.height <= 0 || width <= 0 || height <= 0) {
		return { x: 0, y: 0, zoom: 1 };
	}
	const padX = Math.floor((width - width / (1 + GRAPH_FIT_PADDING)) * 0.5);
	const padY = Math.floor((height - height / (1 + GRAPH_FIT_PADDING)) * 0.5);
	const zoom = clamp(
		Math.min((width - padX * 2) / bounds.width, (height - padY * 2) / bounds.height),
		MIN_GRAPH_ZOOM,
		MAX_GRAPH_ZOOM
	);
	return {
		x: width / 2 - (bounds.x + bounds.width / 2) * zoom,
		y: height / 2 - (bounds.y + bounds.height / 2) * zoom,
		zoom
	};
}

function clamp(value: number, min: number, max: number): number {
	return Math.min(Math.max(value, min), max);
}

/** True when two viewports describe the same view; the tolerance is visual. */
export function sameViewport(a: GraphViewport, b: GraphViewport): boolean {
	const close = (left: number, right: number): boolean => Math.abs(left - right) < 0.001;
	return close(a.x, b.x) && close(a.y, b.y) && close(a.zoom, b.zoom);
}
