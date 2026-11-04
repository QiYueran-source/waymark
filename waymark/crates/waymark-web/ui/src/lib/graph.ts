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
	unresolvedReason,
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
	/**
	 * True when the reader has collapsed this node, so its children are not
	 * drawn. Set by [`collapseGraph`]; a complete graph has none collapsed.
	 */
	collapsed?: boolean;
	/**
	 * How many direct children [`collapseGraph`] hid under this node. It is set
	 * only on a collapsed node, so a reader of the structured alternative can
	 * say how much a folded branch contains without rebuilding the hierarchy.
	 */
	collapsedChildren?: number;
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

/**
 * The card geometry the graph layout shares.
 *
 * A card's width is fixed — the drawing is a column of cards, and a node must
 * not resize with its text — while its height comes from its content. The
 * layout receives each card's measured height from the view; these values are
 * the deterministic geometry before the browser has measured anything.
 */
export const GRAPH_NODE_WIDTH = 216;
/** The shortest card; also the height a card reports before it is measured. */
export const GRAPH_NODE_MIN_HEIGHT = 84;
export const GRAPH_ROW_GAP = 56;
export const GRAPH_COLUMN_GAP = 28;
export const GRAPH_MARGIN = 16;

/** Approximate line boxes inside a card, used only by the fallback estimate. */
const TITLE_LINE_HEIGHT = 18;
const REASON_LINE_HEIGHT = 16;
const CONTEXT_LINE_HEIGHT = 20;
/** Characters a card's inner width fits per line, per text size. */
const TITLE_CHARS_PER_LINE = 30;
const REASON_CHARS_PER_LINE = 40;

function wrappedLineCount(text: string, charsPerLine: number): number {
	return Math.max(1, Math.ceil(text.length / charsPerLine));
}

/**
 * The deterministic card height used until the browser measures the real one.
 *
 * The graph layout must be reproducible without a DOM, so a card starts from a
 * width-aware estimate of its wrapped content and the view replaces it with the
 * measured height when the card reports one. The estimate deliberately only
 * adds full lines: short cards keep the known minimum, and a long title or
 * reason never shrinks the layout below what the text needs.
 */
export function estimatedGraphNodeHeight(node: GraphNode): number {
	let height = GRAPH_NODE_MIN_HEIGHT;
	height += (wrappedLineCount(node.title, TITLE_CHARS_PER_LINE) - 1) * TITLE_LINE_HEIGHT;
	if (node.state === 'blocked' && node.blockedReason !== null) {
		height +=
			(wrappedLineCount(node.blockedReason, REASON_CHARS_PER_LINE) - 1) * REASON_LINE_HEIGHT;
	}
	if (node.context) height += CONTEXT_LINE_HEIGHT;
	return height;
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

/** The zoom range the reader's own gestures and controls stay inside. */
export const MIN_GRAPH_ZOOM = 0.5;
export const MAX_GRAPH_ZOOM = 2;

/**
 * The lowest zoom a fitted viewport may use.
 *
 * A fit must show the whole hierarchy, so it cannot stop at the reader's zoom
 * floor: a large project would overflow while the readout claimed it was all
 * visible. The fit therefore has a much wider floor, and only a drawing too
 * large even for that is reported as not fully in view.
 */
export const MIN_FIT_ZOOM = 0.05;

/** The share of the canvas the fitted drawing leaves free at its edges. */
export const GRAPH_FIT_PADDING = 0.1;

/**
 * The smallest on-screen dash a dependency edge may draw, in screen pixels.
 *
 * `stroke-dasharray` is in drawing coordinates, so the canvas transform scales
 * it with the zoom: a fixed 6/4 pattern is sub-pixel at the default fit for a
 * large project and reads as a solid line. These are the lengths the reader
 * must still see, whatever the zoom. Colour and weight carry the same
 * distinction, so the dash is never the only cue.
 */
export const DEPENDENCY_DASH_SCREEN = 8;
/** The smallest on-screen gap between dashes, in screen pixels. */
export const DEPENDENCY_GAP_SCREEN = 6;

function roundPattern(value: number): number {
	// Round up, never down: the pattern is a floor for the reader's eye, so a
	// rounded-down hundredth must not dip the screen-space dash below the
	// minimum.
	return Math.ceil(value * 100) / 100;
}

/**
 * The dash pattern for a dependency edge, in drawing coordinates, at `zoom`.
 *
 * The pattern is divided by the zoom so its screen-space dash and gap stay at
 * least [`DEPENDENCY_DASH_SCREEN`] and [`DEPENDENCY_GAP_SCREEN`] pixels: the
 * drawn path keeps visible gaps instead of collapsing into a solid line at the
 * default fit of a large drawing. The zoom is clamped to the canvas's own
 * range, so an unusable value (zero, a negative, `NaN` before the first
 * measure) yields the neutral pattern rather than an infinite dash.
 */
export function dependencyDashArray(zoom: number): string {
	const scale =
		Number.isFinite(zoom) && zoom > 0
			? Math.min(Math.max(zoom, MIN_FIT_ZOOM), MAX_GRAPH_ZOOM)
			: 1;
	return `${roundPattern(DEPENDENCY_DASH_SCREEN / scale)} ${roundPattern(
		DEPENDENCY_GAP_SCREEN / scale
	)}`;
}

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
		dependencies: options.dependenciesVisible ? dependencyOverlayEdges(nodes) : [],
		roots,
		unresolved: data.unresolved
	};
}

/** One dashed edge per dependency of a drawn item whose target is drawn too. */
function dependencyOverlayEdges(nodes: GraphNode[]): GraphDependencyEdge[] {
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
 * One corridor of the top-to-bottom layout that several dependencies cross.
 *
 * The overlay in a drawn graph can put more than one dashed line on the same
 * visual route: siblings in one row point at each other along that row, or
 * several children of one generation depend on the same item above or below
 * them. The lines stay separate — this is a count, not an aggregate — so the
 * interface can say how many run there while each one keeps its own label.
 */
export interface DependencyGap {
	/** Stable route key: `row:<depth>` or `gap:<low>:<high>`. */
	key: string;
	/** The gap in words, for the count marker's text. */
	description: string;
	/** How many dependency edges use this corridor. */
	count: number;
	/** The ids of those edges, so each one stays inspectable on its own. */
	edgeIds: string[];
}

/**
 * Count the dependency edges that share each generation gap.
 *
 * Rows come from the model's depth, not from measured pixels: dependencies
 * between two items of one generation travel along that row, and dependencies
 * between different generations cross the corridor between them. The result is
 * ordered by route key, so the same drawing always reports the same gaps.
 */
export function dependencyGaps(graph: WorkGraph): DependencyGap[] {
	const depthById = new Map(graph.nodes.map((node) => [node.id, node.depth]));
	const groups = new Map<string, DependencyGap>();

	for (const edge of graph.dependencies) {
		const from = depthById.get(edge.from);
		const to = depthById.get(edge.to);
		if (from === undefined || to === undefined) continue;
		const low = Math.min(from, to);
		const high = Math.max(from, to);
		const sameRow = low === high;
		const key = sameRow ? `row:${low}` : `gap:${low}:${high}`;
		const description = sameRow
			? `generation ${low} row`
			: `the gap between generations ${low} and ${high}`;
		const existing = groups.get(key);
		if (existing === undefined) {
			groups.set(key, { key, description, count: 1, edgeIds: [edge.id] });
		} else {
			existing.count += 1;
			existing.edgeIds.push(edge.id);
		}
	}

	return [...groups.values()].sort((left, right) =>
		left.key < right.key ? -1 : left.key > right.key ? 1 : 0
	);
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
	/**
	 * The height the layout used for this card: the measured height when the
	 * view has one, otherwise [`estimatedGraphNodeHeight`].
	 */
	height: number;
	/** Report this card's border-box height back to the layout. */
	onMeasure: (id: string, height: number) => void;
	selected: boolean;
	/** True when this node's children are hidden by the reader's collapse. */
	collapsed: boolean;
	onSelect: (id: string, trigger?: HTMLElement) => void;
	/** Collapse or expand this node's children. Never changes the item. */
	onToggle: (id: string) => void;
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
		const across = edge.crossCategory ? ', across categories' : '';
		return `Dependency: ${from?.title ?? edge.from} → ${edge.toTitle} (depends on${across})`;
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
	/**
	 * True when the drawing has this branch folded, so its children are absent
	 * from the alternative too. The entry itself stays, which is what makes the
	 * fold a described row rather than a silent gap.
	 */
	collapsed: boolean;
	/** Direct children the fold hides; 0 when the entry is not collapsed. */
	collapsedChildren: number;
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
 * The graph with the reader's collapsed nodes applied.
 *
 * Collapsing is presentation only: the returned model is a new value, so the
 * complete one survives for anything that still needs it. A node that is itself
 * collapsed stays and is marked `collapsed`; every descendant below it is left
 * out, along with any edge whose far end went with it. Ids in `collapsed` that
 * this drawing does not contain are ignored, which is what lets a stored
 * collapse set survive project changes.
 *
 * This is the density seam: the drawing and the structured text alternative are
 * both built from the result, so a collapsed branch is absent from both rather
 * than from one of them.
 */
export function collapseGraph(graph: WorkGraph, collapsed: ReadonlySet<string>): WorkGraph {
	if (collapsed.size === 0) return graph;

	// `graph.nodes` lists a parent before its children, so one pass suffices: a
	// node below a hidden or collapsed parent is hidden in turn.
	const hidden = new Set<string>();
	const kept: GraphNode[] = [];
	// Count direct children from the complete drawing before anything is hidden:
	// a folded branch still knows how many children its own row stands for.
	const directChildren = new Map<string, number>();
	for (const node of graph.nodes) {
		if (node.parentId === null) continue;
		directChildren.set(node.parentId, (directChildren.get(node.parentId) ?? 0) + 1);
	}
	for (const node of graph.nodes) {
		if (node.parentId !== null && hidden.has(node.parentId)) {
			hidden.add(node.id);
			continue;
		}
		if (collapsed.has(node.id)) {
			hidden.add(node.id);
			kept.push({
				...node,
				collapsed: true,
				collapsedChildren: directChildren.get(node.id) ?? 0
			});
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
 *
 * The lower bound is [`MIN_FIT_ZOOM`], not the reader's [`MIN_GRAPH_ZOOM`]: a
 * fit that cannot contain the drawing is not a fit. [`viewportContains`] answers
 * whether the result really holds every node, so the readout never has to assume
 * it does.
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
		MIN_FIT_ZOOM,
		MAX_GRAPH_ZOOM
	);
	return {
		x: width / 2 - (bounds.x + bounds.width / 2) * zoom,
		y: height / 2 - (bounds.y + bounds.height / 2) * zoom,
		zoom
	};
}

/**
 * Whether `viewport` really shows every corner of `bounds` inside the canvas.
 *
 * This is the claim "whole graph in view", so it is measured rather than
 * inferred from "we called fit": a viewport that was clamped, or one the reader
 * has panned or zoomed away from, is not fitted. A half-pixel tolerance absorbs
 * the rounded margins the fit computes.
 */
export function viewportContains(
	viewport: GraphViewport,
	bounds: GraphBounds | null,
	width: number,
	height: number
): boolean {
	if (bounds === null || width <= 0 || height <= 0) return false;
	const tolerance = 0.5;
	const left = viewport.x + bounds.x * viewport.zoom;
	const top = viewport.y + bounds.y * viewport.zoom;
	const right = viewport.x + (bounds.x + bounds.width) * viewport.zoom;
	const bottom = viewport.y + (bounds.y + bounds.height) * viewport.zoom;
	return (
		left >= -tolerance &&
		top >= -tolerance &&
		right <= width + tolerance &&
		bottom <= height + tolerance
	);
}

function clamp(value: number, min: number, max: number): number {
	return Math.min(Math.max(value, min), max);
}

/** True when two viewports describe the same view; the tolerance is visual. */
export function sameViewport(a: GraphViewport, b: GraphViewport): boolean {
	const close = (left: number, right: number): boolean => Math.abs(left - right) < 0.001;
	return close(a.x, b.x) && close(a.y, b.y) && close(a.zoom, b.zoom);
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
				: null,
			collapsed: node.collapsed === true,
			collapsedChildren: node.collapsed === true ? (node.collapsedChildren ?? 0) : 0
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
