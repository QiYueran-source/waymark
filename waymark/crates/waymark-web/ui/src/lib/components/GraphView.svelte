<script lang="ts">
	/**
	 * The graph presentation: the same active tree, laid out top to bottom by
	 * default or left to right when the reader chooses it.
	 *
	 * This is the only module under `ui/src` that imports the drawing library's
	 * components and the layout engine; its child `GraphBridge.svelte` imports
	 * the library's `useSvelteFlow` hook to hand the pan/zoom API up. Both stay
	 * inside this layer, no dagre or library type escapes it, and the rest of
	 * the app keeps dealing in `WorkGraph` values. The library stylesheet is
	 * imported here too, so the app shell imports no graph library at all.
	 * Placement is computed from the model and the reading direction — one row
	 * or column per generation, with a parent before its children along that
	 * axis — and handed to the renderer as fixed geometry. Nothing here measures
	 * the DOM to decide where a node goes, so the same input always produces the
	 * same drawing.
	 *
	 * Dependency edges are an overlay, not part of the hierarchy: the layout
	 * never sees them, so switching the overlay on cannot move a node. They are
	 * dashed and directed. Whether the model carries any is decided upstream by
	 * `buildGraph`'s `dependenciesVisible`, so this view draws what it is given.
	 *
	 * The view is read-only: no drag, no connect, no delete, and no selection
	 * state of the renderer's own. The only interactive element is the app-owned
	 * title button inside each card.
	 */
	// The graph's structural styles belong to the graph layer, not the app
	// shell: the shell must not import a graph library, even for its CSS.
	import '@xyflow/svelte/dist/base.css';
	import { Graph as DagreGraph, layout as dagreLayout } from '@dagrejs/dagre';
	import { MarkerType, Panel, Position, SvelteFlow } from '@xyflow/svelte';
	import type { Edge, EdgeMarker, Node, NodeTypes } from '@xyflow/svelte';
	import { untrack, type Snippet } from 'svelte';

	import GraphBridge from './GraphBridge.svelte';
	import GraphDirectionControl from './GraphDirection.svelte';
	import GraphItemNode from './GraphItemNode.svelte';
	import UnresolvedList from './UnresolvedList.svelte';
	import {
		GRAPH_COLUMN_GAP,
		GRAPH_MARGIN,
		GRAPH_NODE_WIDTH,
		GRAPH_NOMINAL_CANVAS,
		GRAPH_ROW_GAP,
		DEFAULT_GRAPH_DIRECTION,
		MAX_GRAPH_ZOOM,
		MIN_FIT_ZOOM,
		MIN_GRAPH_ZOOM,
		dependencyDashArray,
		dependencyGaps,
		edgeLabel,
		estimatedGraphNodeHeight,
		fittedViewport,
		graphBounds,
		graphNodeLabel,
		sameViewport,
		viewportContains,
		type GraphDependencyEdge,
		type GraphDirection,
		type GraphFlow,
		type GraphNodePayload,
		type GraphViewport,
		type WorkGraph
	} from '$lib/graph';

	let {
		graph,
		selectedId = null,
		direction = DEFAULT_GRAPH_DIRECTION,
		onDirectionChange,
		onSelect,
		onToggle,
		onExpandAll,
		onCollapseAll,
		restoredViewport = null,
		onViewportChange,
		onViewportReset,
		categoryControl
	}: {
		/**
		 * The drawing to render. Whether the dependency overlay is included is
		 * the model's decision (`buildGraph`'s `dependenciesVisible`), so the view
		 * renders `graph.dependencies` as given.
		 */
		graph: WorkGraph;
		/** The item whose detail panel is open, if any. */
		selectedId?: string | null;
		/**
		 * How the hierarchy is read: top to bottom, or left to right. It changes
		 * the layout and the sides its lines leave and enter by, nothing else.
		 */
		direction?: GraphDirection;
		/** Choosing a direction; the project remembers it. */
		onDirectionChange?: (direction: GraphDirection) => void;
		/**
		 * The category control, supplied by Explorer so the drawing keeps no
		 * category state of its own. It shares the upper-left control cluster.
		 */
		categoryControl?: Snippet;
		/** Selecting a node; the trigger lets focus return after the panel closes. */
		onSelect: (id: string, trigger?: HTMLElement) => void;
		/** Collapsing or expanding a node; the item itself is never changed. */
		onToggle: (id: string) => void;
		/** Graph-wide expand: clear the drawing's collapse set. */
		onExpandAll: () => void;
		/** Graph-wide collapse: fold every drawn node that has children. */
		onCollapseAll: () => void;
		/**
		 * The viewport this project last left, if any. It is applied once, on
		 * entry, instead of fitting; a stored viewport the drawing cannot use is
		 * ignored and the drawing is fitted instead.
		 */
		restoredViewport?: GraphViewport | null;
		/** Called when the reader finishes a pan or zoom, so it can be remembered. */
		onViewportChange?: (viewport: GraphViewport) => void;
		/** Resetting the viewport also forgets the remembered one. */
		onViewportReset?: () => void;
	} = $props();

	/**
	 * The edge the pointer is over, if any.
	 *
	 * Hover is drawing state only: it raises and thickens one line and prints
	 * its relation beside it, so a reader can tell which relation a line means
	 * where several share the same gap. It never selects an edge or changes the
	 * model, and the edge's accessible name is present without hovering.
	 */
	let hoveredEdgeId: string | null = $state(null);

	/**
	 * The card geometry lives with the model (`$lib/graph.ts`), so the layout and
	 * the fallback height estimate cannot drift apart: a card's width is fixed,
	 * and its height is laid out per node.
	 */
	const NODE_WIDTH = GRAPH_NODE_WIDTH;
	const ROW_GAP = GRAPH_ROW_GAP;
	const COLUMN_GAP = GRAPH_COLUMN_GAP;
	const MARGIN = GRAPH_MARGIN;

	/**
	 * Handle ids. The hierarchy always leaves the bottom of a parent and enters
	 * the top of its child; the overlay picks the facing pair instead, so a
	 * dependency can point upwards or sideways without crossing the node.
	 */
	const OUT_BOTTOM = 'out-bottom';
	const IN_TOP = 'in-top';
	const OUT_TOP = 'out-top';
	const IN_BOTTOM = 'in-bottom';
	const OUT_RIGHT = 'out-right';
	const IN_LEFT = 'in-left';
	const OUT_LEFT = 'out-left';
	const IN_RIGHT = 'in-right';

	/**
	 * Parallel handles for a dependency that covers the same pair as a
	 * parent-child edge. Each is shifted along the card's side, so the dashed
	 * line runs beside the solid one instead of exactly under it.
	 */
	const HANDLE_OFFSET = 14;
	const OUT_BOTTOM_OFFSET = 'out-bottom-offset';
	const IN_TOP_OFFSET = 'in-top-offset';
	const OUT_TOP_OFFSET = 'out-top-offset';
	const IN_BOTTOM_OFFSET = 'in-bottom-offset';
	const OUT_RIGHT_OFFSET = 'out-right-offset';
	const IN_LEFT_OFFSET = 'in-left-offset';
	const OUT_LEFT_OFFSET = 'out-left-offset';
	const IN_RIGHT_OFFSET = 'in-right-offset';

	/**
	 * The overlay's arrowhead, defined by the renderer's marker machinery and
	 * referenced from the edge path's target end. It is the one app-owned hook
	 * that makes the direction visible at a glance.
	 */
	const DEPENDENCY_MARKER: EdgeMarker = {
		type: MarkerType.ArrowClosed,
		color: 'var(--dep-edge)',
		width: 14,
		height: 14
	};

	/**
	 * The two line treatments are painted explicitly on the path, not left to
	 * the renderer's defaults: hierarchy is a guide-coloured solid stroke,
	 * dependency is a thicker blue-grey dashed one. Colour, weight, and dash all
	 * differ, so the kinds never rest on colour alone. Each kind names both of
	 * its weights, and a raised one is thicker than the same kind at rest.
	 */
	const PARENT_CHILD_WEIGHT = 1.5;
	const PARENT_CHILD_RAISED_WEIGHT = 2.5;
	const DEPENDENCY_WEIGHT = 2.5;
	const DEPENDENCY_RAISED_WEIGHT = 3.5;
	const RAISED_Z_INDEX = 1000;

	function edgeStyle(
		kind: 'parent-child' | 'dependency',
		zoom: number,
		raised: boolean,
		screenSpan?: number
	): string {
		const width =
			kind === 'dependency'
				? raised
					? DEPENDENCY_RAISED_WEIGHT
					: DEPENDENCY_WEIGHT
				: raised
					? PARENT_CHILD_RAISED_WEIGHT
					: PARENT_CHILD_WEIGHT;
		if (kind === 'parent-child') return `stroke: var(--guide); stroke-width: ${width};`;
		return `stroke: var(--dep-edge); stroke-width: ${width}; stroke-dasharray: ${dependencyDashArray(
			zoom,
			screenSpan
		)};`;
	}

	function edgeClass(kind: 'parent-child' | 'dependency'): string {
		return `graph-edge graph-edge-${kind}`;
	}

	interface PlacedNode {
		payload: GraphNodePayload;
		x: number;
		y: number;
		height: number;
	}

	/**
	 * Real card heights reported by the rendered cards, by node id.
	 *
	 * The layout is a pure function of the model plus this map, so it can be
	 * computed before the browser paints (from the deterministic estimate) and
	 * again when cards report their measured height. Nothing elsewhere reads the
	 * DOM to place a node.
	 */
	let measuredHeights: Record<string, number> = $state({});

	/**
	 * Take one card's measured border-box height back from the renderer.
	 *
	 * A card reports through a `ResizeObserver`, so this runs once per real
	 * change: the map keeps only whole pixels and ignores a repeated or empty
	 * measurement, which is what keeps the re-layout from looping.
	 */
	function measureNode(id: string, height: number): void {
		const measured = Math.round(height);
		if (!Number.isFinite(measured) || measured <= 0) return;
		if (measuredHeights[id] === measured) return;
		measuredHeights = { ...measuredHeights, [id]: measured };
	}

	/**
	 * Lay the hierarchy out in the chosen direction.
	 *
	 * Every root of the walk starts its own tree; the engine puts each node
	 * after its parent along the reading direction, so depth and position agree
	 * by construction. In `TB` a generation is a row and a parent sits above its
	 * children; in `LR` a generation is a column and a parent sits to their
	 * left. Siblings stack along the other axis. Work-item ids are passed
	 * through unchanged — the graph never renames, nor wraps an item in a
	 * synthetic node — and only solid parent-child relations become layout
	 * edges. The dashed overlay is deliberately absent here.
	 *
	 * Each card's own height is handed to the engine, so a taller card pushes
	 * its neighbours along the stacking axis and no edge has to be drawn through
	 * wrapped text.
	 */
	function placeNodes(
		model: WorkGraph,
		heights: Readonly<Record<string, number>>,
		direction: GraphDirection
	): PlacedNode[] {
		if (model.nodes.length === 0) return [];

		const layoutGraph = new DagreGraph();
		layoutGraph.setGraph({
			rankdir: direction,
			nodesep: COLUMN_GAP,
			ranksep: ROW_GAP,
			marginx: MARGIN,
			marginy: MARGIN
		});
		// The engine reads an edge label for every edge it creates, so it needs a
		// factory; without one it throws before laying anything out.
		layoutGraph.setDefaultEdgeLabel(() => ({}));

		for (const node of model.nodes) {
			layoutGraph.setNode(node.id, {
				width: NODE_WIDTH,
				height: heights[node.id] ?? estimatedGraphNodeHeight(node)
			});
		}
		for (const edge of model.edges) {
			layoutGraph.setEdge(edge.from, edge.to);
		}

		dagreLayout(layoutGraph);

		return model.nodes.map((node) => {
			const center = layoutGraph.node(node.id) as { x: number; y: number } | undefined;
			const height = heights[node.id] ?? estimatedGraphNodeHeight(node);
			const x = (center?.x ?? 0) - NODE_WIDTH / 2;
			const y = (center?.y ?? 0) - height / 2;
			return {
				payload: {
					node,
					graphX: x,
					graphY: y,
					height,
					selected: node.id === selectedId,
					collapsed: node.collapsed === true,
					onSelect,
					onToggle,
					onMeasure: measureNode
				},
				x,
				y,
				height
			};
		});
	}

	const placed = $derived(placeNodes(graph, measuredHeights, direction));

	/**
	 * Where each node sits, so an overlay can leave by the nearest side and can
	 * measure the straight chord between two cards.
	 */
	const placedById = $derived(
		new Map(
			placed.map((entry) => [
				entry.payload.node.id,
				{ x: entry.x, y: entry.y, height: entry.height, depth: entry.payload.node.depth }
			])
		)
	);

	/**
	 * The card pairs the hierarchy already joins, in either direction.
	 *
	 * A dependency between the same two items is still its own relation, but it
	 * must not be painted exactly under the solid parent-child line; the
	 * overlay checks this set and takes the offset handles when it matches.
	 */
	const parentPairs = $derived(
		new Set(
			graph.edges.map((edge) =>
				edge.from < edge.to ? `${edge.from}|${edge.to}` : `${edge.to}|${edge.from}`
			)
		)
	);

	/**
	 * How many dependencies share each generation gap.
	 *
	 * The lines themselves stay separate — nothing is aggregated — but a reader
	 * who sees several dashes in one corridor can see the count and hover each
	 * line for its relation. Only a shared gap is announced.
	 */
	const gapGroups = $derived(dependencyGaps(graph, direction));
	const sharedGaps = $derived(gapGroups.filter((gap) => gap.count > 1));
	const gapByEdge = $derived(
		new Map(gapGroups.flatMap((gap) => gap.edgeIds.map((edgeId) => [edgeId, gap] as const)))
	);

	/**
	 * The sides a parent-child edge leaves and enters in the current direction.
	 *
	 * The hierarchy's relation itself is unchanged; only which sides face each
	 * other. In `TB` the child is below its parent, so the line leaves the
	 * bottom and enters the top. In `LR` the child is to the right, so it leaves
	 * the right and enters the left.
	 */
	function hierarchyHandles(direction: GraphDirection): { source: string; target: string } {
		return direction === 'LR'
			? { source: OUT_RIGHT, target: IN_LEFT }
			: { source: OUT_BOTTOM, target: IN_TOP };
	}

	/**
	 * Which pair of handles a dependency edge should use.
	 *
	 * A dependency keeps its own direction; only its route depends on the
	 * reading direction. In `TB`, a generation is a row: most dependencies point
	 * up or down and take the vertical pair, while two items on the same row —
	 * two roots, or a dependency between siblings — are drawn side by side and
	 * take the horizontal pair. In `LR` the axes swap: a generation is a column,
	 * so same-generation items are stacked and take the vertical pair, while a
	 * cross-generation dependency leaves through the side facing its target.
	 *
	 * Rows and columns are identified by depth, not by the top edge: cards in one
	 * generation can differ in height and therefore in where their edges sit. A
	 * coincident relation takes the offset pair, so both lines stay visible.
	 */
	function dependencyHandles(
		from: string,
		to: string,
		coincident: boolean,
		direction: GraphDirection
	): { source: string; target: string } {
		const source = placedById.get(from);
		const target = placedById.get(to);
		if (source === undefined || target === undefined) {
			return { source: OUT_BOTTOM, target: IN_TOP };
		}
		const sameGeneration = source.depth === target.depth;
		// The axis a dependency travels on is the one along which the two ends
		// are separated: the stacking axis for one generation, the reading axis
		// between generations.
		const vertical = direction === 'LR' ? sameGeneration : !sameGeneration;
		if (vertical) {
			if (target.y >= source.y) {
				return coincident
					? { source: OUT_BOTTOM_OFFSET, target: IN_TOP_OFFSET }
					: { source: OUT_BOTTOM, target: IN_TOP };
			}
			return coincident
				? { source: OUT_TOP_OFFSET, target: IN_BOTTOM_OFFSET }
				: { source: OUT_TOP, target: IN_BOTTOM };
		}
		if (target.x >= source.x) {
			return coincident
				? { source: OUT_RIGHT_OFFSET, target: IN_LEFT_OFFSET }
				: { source: OUT_RIGHT, target: IN_LEFT };
		}
		return coincident
			? { source: OUT_LEFT_OFFSET, target: IN_RIGHT_OFFSET }
			: { source: OUT_LEFT, target: IN_RIGHT };
	}

	/**
	 * The nodes handed to the renderer.
	 *
	 * `draggable`/`selectable`/`connectable`/`focusable` are all off: the graph
	 * is a drawing, and the only tab stop per node is the card's own button.
	 * The static handles let the renderer anchor the solid edges without
	 * measuring anything; the extra vertical pair is used only by the overlay,
	 * so a dependency arrow can approach from the side the target is on.
	 */
	const nodes: Node[] = $derived(
		placed.map(({ payload, x, y, height }) => ({
			id: payload.node.id,
			type: 'workitem',
			position: { x, y },
			width: NODE_WIDTH,
			height,
			draggable: false,
			selectable: false,
			connectable: false,
			focusable: false,
			ariaLabel: graphNodeLabel(payload.node),
			data: payload,
			handles: [
				{
					id: OUT_BOTTOM,
					type: 'source',
					position: Position.Bottom,
					x: NODE_WIDTH / 2,
					y: height
				},
				{ id: IN_TOP, type: 'target', position: Position.Top, x: NODE_WIDTH / 2, y: 0 },
				{ id: OUT_TOP, type: 'source', position: Position.Top, x: NODE_WIDTH / 2, y: 0 },
				{
					id: IN_BOTTOM,
					type: 'target',
					position: Position.Bottom,
					x: NODE_WIDTH / 2,
					y: height
				},
				{
					id: OUT_RIGHT,
					type: 'source',
					position: Position.Right,
					x: NODE_WIDTH,
					y: height / 2
				},
				{ id: IN_LEFT, type: 'target', position: Position.Left, x: 0, y: height / 2 },
				{ id: OUT_LEFT, type: 'source', position: Position.Left, x: 0, y: height / 2 },
				{
					id: IN_RIGHT,
					type: 'target',
					position: Position.Right,
					x: NODE_WIDTH,
					y: height / 2
				},
				// The offset pair used by a dependency that coincides with a
				// parent-child edge: same sides, shifted so both lines show.
				{
					id: OUT_BOTTOM_OFFSET,
					type: 'source',
					position: Position.Bottom,
					x: NODE_WIDTH / 2 + HANDLE_OFFSET,
					y: height
				},
				{
					id: IN_TOP_OFFSET,
					type: 'target',
					position: Position.Top,
					x: NODE_WIDTH / 2 + HANDLE_OFFSET,
					y: 0
				},
				{
					id: OUT_TOP_OFFSET,
					type: 'source',
					position: Position.Top,
					x: NODE_WIDTH / 2 + HANDLE_OFFSET,
					y: 0
				},
				{
					id: IN_BOTTOM_OFFSET,
					type: 'target',
					position: Position.Bottom,
					x: NODE_WIDTH / 2 + HANDLE_OFFSET,
					y: height
				},
				{
					id: OUT_RIGHT_OFFSET,
					type: 'source',
					position: Position.Right,
					x: NODE_WIDTH,
					y: height / 2 + HANDLE_OFFSET
				},
				{
					id: IN_LEFT_OFFSET,
					type: 'target',
					position: Position.Left,
					x: 0,
					y: height / 2 + HANDLE_OFFSET
				},
				{
					id: OUT_LEFT_OFFSET,
					type: 'source',
					position: Position.Left,
					x: 0,
					y: height / 2 + HANDLE_OFFSET
				},
				{
					id: IN_RIGHT_OFFSET,
					type: 'target',
					position: Position.Right,
					x: NODE_WIDTH,
					y: height / 2 + HANDLE_OFFSET
				}
			],
			domAttributes: {
				'data-node-id': payload.node.id,
				'data-depth': String(payload.node.depth),
				'data-parent-id': payload.node.parentId ?? ''
			}
		}))
	);

	/**
	 * One solid edge per parent-child relation, named for assistive technology.
	 * A dependency relation between the same two items is a separate edge and
	 * never replaces this one.
	 */
	const hierarchyEdges: Edge[] = $derived(
		graph.edges.map((edge) => {
			const raised = hoveredEdgeId === edge.id;
			const handles = hierarchyHandles(direction);
			return {
				id: edge.id,
				source: edge.from,
				target: edge.to,
				sourceHandle: handles.source,
				targetHandle: handles.target,
				style: edgeStyle('parent-child', 1, raised),
				class: edgeClass('parent-child'),
				zIndex: raised ? RAISED_Z_INDEX : 0,
				label: raised ? edgeLabel(graph, edge) : undefined,
				ariaLabel: edgeLabel(graph, edge),
				domAttributes: {
					'data-testid': 'graph-edge',
					'data-from': edge.from,
					'data-to': edge.to,
					'data-edge-kind': 'parent-child',
					'data-hovered': raised ? 'true' : 'false',
					'data-source-handle': handles.source,
					'data-target-handle': handles.target
				}
			};
		})
	);

	/**
	 * Where one handle sits on a placed card, in drawing coordinates.
	 *
	 * The four sides are the base geometry; the offset pair used by a
	 * coincident relation shifts along the side, matching the static handle
	 * metadata handed to the renderer. These points are what an edge actually
	 * spans, which can be much shorter than the distance between card centres:
	 * two adjacent cards in one row are joined only across the column gap.
	 */
	function handlePoint(
		entry: { x: number; y: number; height: number },
		handle: string
	): { x: number; y: number } {
		const left = entry.x;
		const right = entry.x + NODE_WIDTH;
		const top = entry.y;
		const bottom = entry.y + entry.height;
		const midX = entry.x + NODE_WIDTH / 2;
		const midY = entry.y + entry.height / 2;
		switch (handle) {
			case OUT_BOTTOM:
				return { x: midX, y: bottom };
			case IN_TOP:
				return { x: midX, y: top };
			case OUT_TOP:
				return { x: midX, y: top };
			case IN_BOTTOM:
				return { x: midX, y: bottom };
			case OUT_RIGHT:
				return { x: right, y: midY };
			case IN_LEFT:
				return { x: left, y: midY };
			case OUT_LEFT:
				return { x: left, y: midY };
			case IN_RIGHT:
				return { x: right, y: midY };
			case OUT_BOTTOM_OFFSET:
				return { x: midX + HANDLE_OFFSET, y: bottom };
			case IN_TOP_OFFSET:
				return { x: midX + HANDLE_OFFSET, y: top };
			case OUT_TOP_OFFSET:
				return { x: midX + HANDLE_OFFSET, y: top };
			case IN_BOTTOM_OFFSET:
				return { x: midX + HANDLE_OFFSET, y: bottom };
			case OUT_RIGHT_OFFSET:
				return { x: right, y: midY + HANDLE_OFFSET };
			case IN_LEFT_OFFSET:
				return { x: left, y: midY + HANDLE_OFFSET };
			case OUT_LEFT_OFFSET:
				return { x: left, y: midY + HANDLE_OFFSET };
			case IN_RIGHT_OFFSET:
				return { x: right, y: midY + HANDLE_OFFSET };
			default:
				return { x: midX, y: midY };
		}
	}

	/**
	 * The straight-line screen distance between the two handles an edge joins.
	 *
	 * A dependency between adjacent generations spans only about one row gap,
	 * and two cards in one row are joined only across the column gap; at a low
	 * fit zoom either can be shorter on screen than one whole dash-plus-gap and
	 * would draw as a single solid dash. The dash helper needs that chord to cap
	 * the pattern. An edge whose ends are not both placed has no chord, which
	 * keeps the ordinary zoom-only pattern.
	 */
	function dependencyScreenSpan(
		from: string,
		to: string,
		handles: { source: string; target: string },
		zoom: number
	): number | undefined {
		const source = placedById.get(from);
		const target = placedById.get(to);
		if (source === undefined || target === undefined) return undefined;
		const start = handlePoint(source, handles.source);
		const end = handlePoint(target, handles.target);
		return Math.hypot(start.x - end.x, start.y - end.y) * zoom;
	}

	/**
	 * One dashed, directed edge per dependency, arrow pointing at the item
	 * depended on. `data-arrow="target"` is the app-owned hook that states the
	 * direction without geometry; the accessible name says it in words too.
	 * Its dash is capped by the edge's own screen span, so a short chord keeps
	 * a visible gap even at the fitted zoom of a large drawing.
	 */
	function dependencyEdge(edge: GraphDependencyEdge, zoom: number): Edge {
		const coincident = parentPairs.has(
			edge.from < edge.to ? `${edge.from}|${edge.to}` : `${edge.to}|${edge.from}`
		);
		const handles = dependencyHandles(edge.from, edge.to, coincident, direction);
		const gap = gapByEdge.get(edge.id);
		const raised = hoveredEdgeId === edge.id;
		const screenSpan = dependencyScreenSpan(edge.from, edge.to, handles, zoom);
		return {
			id: edge.id,
			source: edge.from,
			target: edge.to,
			sourceHandle: handles.source,
			targetHandle: handles.target,
			style: edgeStyle('dependency', zoom, raised, screenSpan),
			class: edgeClass('dependency'),
			zIndex: raised ? RAISED_Z_INDEX : 0,
			label: raised ? edgeLabel(graph, edge) : undefined,
			markerEnd: DEPENDENCY_MARKER,
			ariaLabel: edgeLabel(graph, edge),
			domAttributes: {
				'data-testid': 'graph-edge',
				'data-from': edge.from,
				'data-to': edge.to,
				'data-edge-kind': 'dependency',
				'data-arrow': 'target',
				'data-hovered': raised ? 'true' : 'false',
				'data-coincident': coincident ? 'true' : 'false',
				'data-gap': gap?.key ?? '',
				'data-gap-count': String(gap?.count ?? 1),
				'data-source-handle': handles.source,
				'data-target-handle': handles.target
			}
		};
	}

	const nodeTypes: NodeTypes = { workitem: GraphItemNode };

	/**
	 * The navigation layer.
	 *
	 * The drawing's box is measured by the browser, so the fitted viewport is
	 * computed here from the model's geometry and that box: the same drawing
	 * always fits the same way, and a viewport restored from storage can be
	 * compared with the fit exactly. Until the box has a size — before mount,
	 * and in a DOM without layout — a nominal canvas stands in, so the first
	 * entry is still a fit rather than the unzoomed corner of the drawing.
	 */
	let canvasBox: HTMLElement | undefined = $state();

	/** The pan/zoom API, once the drawing has mounted and handed it over. */
	let flow: GraphFlow | null = $state(null);

	/**
	 * The drawing box's measured size.
	 *
	 * It is state, not a DOM read in a derivation, because the canvas can change
	 * size without the model changing: the window resizes, or the detail panel
	 * takes its width. A fresh fit is then computed against the canvas that
	 * remains.
	 */
	let canvasSize = $state({ width: 0, height: 0 });

	/** The last values taken from the DOM, so a measurement is recorded once. */
	let lastCanvasSize = { width: 0, height: 0 };

	/**
	 * Take the drawing box's size from the DOM.
	 *
	 * Recording the size never applies a viewport: a resize preserves the
	 * reader's pan and zoom, and fitting stays an explicit action.
	 */
	function measureCanvas(): void {
		const box = canvasBox;
		if (box === undefined) return;
		const width = box.clientWidth;
		const height = box.clientHeight;
		if (width === lastCanvasSize.width && height === lastCanvasSize.height) return;
		lastCanvasSize = { width, height };
		canvasSize = { width, height };
	}

	/** How wide the drawing's box is, or the nominal canvas if it has no size. */
	function canvasWidth(): number {
		return canvasSize.width > 0 ? canvasSize.width : GRAPH_NOMINAL_CANVAS.width;
	}

	function canvasHeight(): number {
		return canvasSize.height > 0 ? canvasSize.height : GRAPH_NOMINAL_CANVAS.height;
	}

	// Follow the box the drawing actually fills. The observer only re-measures;
	// it never re-applies the viewport, so resizing cannot lose the reader's
	// place. The window listener covers a DOM without ResizeObserver delivery.
	$effect(() => {
		const box = canvasBox;
		if (box === undefined) return;
		measureCanvas();
		const observer =
			typeof ResizeObserver === 'function' ? new ResizeObserver(() => measureCanvas()) : null;
		observer?.observe(box);
		window.addEventListener('resize', measureCanvas);
		return () => {
			observer?.disconnect();
			window.removeEventListener('resize', measureCanvas);
		};
	});

	/** Where the whole visible drawing sits, in drawing coordinates. */
	const bounds = $derived(
		graphBounds(placed.map(({ x, y, height }) => ({ x, y, width: NODE_WIDTH, height })))
	);

	/** The viewport that shows the whole visible drawing, centred. */
	const fitted = $derived(fittedViewport(bounds, canvasWidth(), canvasHeight()));

	/**
	 * The floor for the reader's own zoom gestures and controls.
	 *
	 * Normally [`MIN_GRAPH_ZOOM`]. When the drawing is so large that even its
	 * fit is below that floor, the floor would make the first Zoom out press
	 * clamp *up* (zoom in); the reader's range is then opened down to the fit
	 * floor instead, which is still a bound and never a jump.
	 */
	const zoomFloor = $derived(
		fitted.zoom < MIN_GRAPH_ZOOM ? MIN_FIT_ZOOM : MIN_GRAPH_ZOOM
	);

	/** The viewport this drawing entered at; the state "reset" returns to. */
	let initial: GraphViewport | null = $state(null);

	/**
	 * The estimate the entry fit was computed from, and the node set it fitted.
	 *
	 * A first entry fits before paint, from the deterministic estimates. When
	 * the cards then report their real heights, the fitted view moves; these
	 * keep the entry fit honest by re-fitting only while the reader has not
	 * moved and the drawn set has not changed. A restored viewport is never
	 * replaced, and folding or filtering never auto-fits.
	 */
	let entryMeasurements: string | null = null;
	let entryNodeIds: string | null = null;
	let entryWasFit = false;

	function measurementSignature(heights: Readonly<Record<string, number>>): string {
		return Object.keys(heights)
			.sort()
			.map((id) => `${id}:${heights[id]}`)
			.join('|');
	}

	function nodeSignature(model: WorkGraph): string {
		return model.nodes.map((node) => node.id).join('|');
	}

	/**
	 * The canvas's own transform, bound through the drawing.
	 *
	 * Every pan, wheel, pinch, and control moves this value, so the readout and
	 * the "is it fitted" question are answered by what the canvas actually
	 * shows. It is unset only until this view enters, because the drawing
	 * reports a transform only once it has mounted and been given one.
	 */
	let canvasViewport: GraphViewport | undefined = $state(undefined);

	/**
	 * The transform this view asked for last, and has not yet seen reported.
	 *
	 * The drawing announces the end of every transform, including one it was
	 * told to make, so this is what tells a reader's gesture from this view's
	 * own move: only the former is worth remembering.
	 */
	let requested: GraphViewport | null = null;

	/** What the reader sees now. */
	const shown: GraphViewport = $derived(canvasViewport ?? initial ?? fitted);

	/**
	 * Every edge the renderer draws: the hierarchy first, then the dependency
	 * overlay. The dash pattern is recomputed from the shown zoom, so a zoom
	 * gesture cannot shrink it below what the reader can see.
	 */
	const edges: Edge[] = $derived([
		...hierarchyEdges,
		...graph.dependencies.map((edge) => dependencyEdge(edge, shown.zoom))
	]);

	/**
	 * True while the canvas shows the fitted view and that view really contains
	 * every drawn node.
	 *
	 * Both halves matter: a pan is not the fit even when the cards still happen
	 * to be on screen, and a fit that had to clamp (a drawing larger than any
	 * usable zoom can contain) is not the whole graph. This is measured against
	 * the drawing's bounds rather than inferred from "we called fit".
	 */
	const showsWholeGraph = $derived(
		sameViewport(shown, fitted) &&
			viewportContains(shown, bounds, canvasWidth(), canvasHeight())
	);

	/**
	 * Move the drawing, and record where it was moved to.
	 *
	 * The record is written here rather than awaited from the drawing so the
	 * readout, the fitted marker, and the stored preference move together, with
	 * the drawing's own report confirming them a moment later.
	 */
	function apply(viewport: GraphViewport): void {
		requested = viewport;
		canvasViewport = viewport;
		void flow?.setViewport(viewport);
	}

	/**
	 * Enter at the remembered viewport when this project has a usable one, and
	 * fit otherwise.
	 *
	 * Once per mounted drawing: re-entering after the reader has moved would
	 * yank the view out from under them, so nothing re-applies it later.
	 */
	function enterViewport(): void {
		if (flow === null || initial !== null) return;
		// Measure before fitting: entering with the detail panel already open
		// fits the width that is actually left, not the one from before it.
		measureCanvas();
		initial = restoredViewport ?? fitted;
		entryWasFit = restoredViewport === null;
		entryMeasurements = measurementSignature(measuredHeights);
		entryNodeIds = nodeSignature(graph);
		apply(initial);
	}

	// The drawing hands over its pan/zoom API through `GraphBridge`, and reports
	// `oninit` once that API is ready; either signal is enough to enter.
	$effect(() => {
		if (flow !== null) enterViewport();
	});

	/**
	 * Whether the detail panel was open the last time this effect looked, or
	 * `null` until the first look. Starting unknown means entering the graph
	 * with a detail already open is not mistaken for the panel just opening.
	 */
	let detailWasOpen: boolean | null = null;

	/**
	 * Fit the remaining canvas when the detail panel opens.
	 *
	 * The detail is a side panel, so opening it takes width from the drawing and
	 * the whole graph must be fitted to what is left. The panel is already in
	 * the DOM when this effect runs, so reading the box measures the reduced
	 * canvas. Closing does not fit: the drawing stays where the reader left it,
	 * and panning afterwards is the reader's own view.
	 */
	$effect(() => {
		const open = selectedId !== null;
		if (detailWasOpen === null) {
			detailWasOpen = open;
			return;
		}
		if (open === detailWasOpen) return;
		detailWasOpen = open;
		if (!open) return;
		untrack(() => {
			measureCanvas();
			apply(fitted);
		});
	});

	/**
	 * The direction the drawing was last laid out in, so an explicit switch can
	 * be told apart from the entry that already had this direction.
	 */
	let appliedDirection: GraphDirection | null = null;

	/**
	 * Fit the visible drawing when the reader explicitly switches direction.
	 *
	 * A switch transposes the layout, so the transform from the other
	 * orientation no longer shows the graph; this is the one layout change that
	 * refits, alongside first entry. The detail panel is already in the DOM
	 * when this runs, so the box measured here is the remaining canvas. The
	 * resulting fit is reported as the remembered viewport for the new
	 * direction, so a reload restores it instead of a record that would show a
	 * different part of the transposed drawing.
	 */
	$effect(() => {
		const current = direction;
		if (appliedDirection === null) {
			appliedDirection = current;
			return;
		}
		if (current === appliedDirection) return;
		appliedDirection = current;
		untrack(() => {
			measureCanvas();
			const fit = fitted;
			initial = fit;
			entryWasFit = true;
			entryMeasurements = measurementSignature(measuredHeights);
			entryNodeIds = nodeSignature(graph);
			apply(fit);
			onViewportChange?.(fit);
		});
	});

	/**
	 * Re-fit a first entry once the real card heights replace the estimates.
	 *
	 * Only the transition from estimated to measured geometry triggers this, and
	 * only while the canvas is still where entry left it for the same node set.
	 * A reader's pan or zoom, a restored viewport, and a changed graph all opt
	 * out, so measurement can never yank a view the reader owns.
	 */
	$effect(() => {
		const measurements = measurementSignature(measuredHeights);
		const fit = fitted;
		if (flow === null || initial === null || !entryWasFit) return;
		if (nodeSignature(graph) !== entryNodeIds) return;
		if (measurements === entryMeasurements) return;
		if (!sameViewport(shown, initial)) return;
		entryMeasurements = measurements;
		initial = fit;
		apply(fit);
	});

	/** Fit the whole visible drawing, and remember that as the view. */
	function fitToView(): void {
		apply(fitted);
		onViewportChange?.(fitted);
	}

	/**
	 * Return to the state a first entry produces: the fitted drawing.
	 *
	 * The remembered pan and zoom are forgotten with it, so a reload starts
	 * fitted rather than at the transform the reader has just discarded.
	 */
	function resetViewport(): void {
		apply(fitted);
		onViewportReset?.();
	}

	function zoomIn(): void {
		void flow?.zoomIn();
	}

	function zoomOut(): void {
		void flow?.zoomOut();
	}

	/**
	 * The reader finished a gesture: remember where the drawing was left.
	 *
	 * A transform this view asked for is not the reader's choice, so it is not
	 * written back; the store would only be told what it already holds.
	 */
	function handleMoveEnd(_event: unknown, viewport: GraphViewport): void {
		if (requested !== null && sameViewport(viewport, requested)) {
			requested = null;
			return;
		}
		requested = null;
		onViewportChange?.(viewport);
	}

	/** A readout is not a measurement log: three decimals is enough for both. */
	function round(value: number, places = 1): string {
		return String(Math.round(value * 10 ** places) / 10 ** places);
	}

</script>

<div class="graph-pane" data-testid="graph" data-direction={direction}>
	<!-- The box the fit is measured against: the drawing fills it, and the
	     unresolved drawer below takes its own row instead of overlaying it. -->
	<div class="graph-canvas-host" data-testid="graph-canvas" bind:this={canvasBox}>
		<SvelteFlow
			{nodes}
			{edges}
			{nodeTypes}
			class="graph-canvas"
			nodesDraggable={false}
			nodesConnectable={false}
			elementsSelectable={false}
			nodesFocusable={false}
			edgesFocusable={false}
			deleteKey={null}
			zoomOnDoubleClick={false}
			panOnDrag
			panOnScroll
			zoomOnScroll={false}
			zoomOnPinch
			minZoom={zoomFloor}
			maxZoom={MAX_GRAPH_ZOOM}
			bind:viewport={canvasViewport}
			oninit={enterViewport}
			onmoveend={handleMoveEnd}
			onedgepointerenter={({ edge }) => (hoveredEdgeId = edge.id)}
			onedgepointerleave={({ edge }) => {
				if (hoveredEdgeId === edge.id) hoveredEdgeId = null;
			}}
			proOptions={{ hideAttribution: true }}
		>
			<GraphBridge onReady={(handle) => (flow = handle)} />

			<!-- The controls float together at the canvas's upper-left: the
			     category picker, the canvas row, and the direction choice. -->
			<Panel position="top-left">
				<div class="graph-controls" data-testid="graph-controls">
					<div class="graph-controls-cluster" data-testid="graph-controls-cluster">
						{#if categoryControl}
							{@render categoryControl()}
						{/if}
						<!-- The reading direction: it only transposes the layout. -->
						{#if onDirectionChange}
							<GraphDirectionControl {direction} onChange={onDirectionChange} />
						{/if}
						<!-- Graph-wide density: presentation only, on the graph's own
						     collapse set. The tree's pair never appears here. -->
						<div class="graph-controls-row" role="group" aria-label="Expansion">
							<button type="button" data-testid="graph-expand-all" onclick={onExpandAll}>
								Expand all
							</button>
							<button type="button" data-testid="graph-collapse-all" onclick={onCollapseAll}>
								Collapse all
							</button>
						</div>
						<!-- Pan and zoom are the canvas's own gestures: dragging the
						     background, the wheel, a trackpad pinch, or two fingers on a
						     touch screen. These controls only repeat or undo them. -->
						<div class="graph-controls-row" role="group" aria-label="Canvas">
							<button type="button" data-testid="graph-zoom-out" onclick={zoomOut}>
								Zoom out
							</button>
							<button type="button" data-testid="graph-zoom-in" onclick={zoomIn}>Zoom in</button>
							<button type="button" data-testid="graph-fit" onclick={fitToView}>Fit to view</button>
							<button type="button" data-testid="graph-reset" onclick={resetViewport}>
								Reset viewport
							</button>
						</div>
					</div>
					<p
						class="graph-readout"
						data-testid="graph-viewport"
						data-x={round(shown.x)}
						data-y={round(shown.y)}
						data-zoom={round(shown.zoom, 3)}
						data-fitted={showsWholeGraph ? 'true' : 'false'}
						role="status"
					>
						Zoom {Math.round(shown.zoom * 100)}% · {showsWholeGraph
							? 'whole graph in view'
							: 'panned'}
					</p>
				</div>
			</Panel>

			<Panel position="top-right">
				<div class="graph-legend" data-testid="graph-legend">
					<span class="legend-entry" data-legend-kind="parent-child">
						<svg class="legend-edge" viewBox="0 0 24 8" aria-hidden="true" focusable="false">
							<line x1="1" y1="4" x2="23" y2="4" />
						</svg>
						<span>Solid line: parent-child</span>
					</span>
					<span class="legend-entry" data-legend-kind="dependency">
						<svg
							class="legend-edge dashed"
							viewBox="0 0 24 8"
							aria-hidden="true"
							focusable="false"
						>
							<line x1="1" y1="4" x2="16" y2="4" stroke-dasharray="4 3" />
							<polygon points="16,1 23,4 16,7" />
						</svg>
						<span>Dashed line: dependency; the arrow points at the depended-on item</span>
					</span>
					<p class="legend-hint" data-testid="graph-legend-hint">
						Hover an edge to identify its relation.
					</p>
				</div>
			</Panel>

			{#if sharedGaps.length > 0}
				<!-- Shared corridors stay separate lines; this marker only states
				     how many run there and how to inspect one. -->
				<Panel position="bottom-left">
					<div class="graph-gap-counts" data-testid="graph-gap-counts">
						{#each sharedGaps as gap (gap.key)}
							<p
								class="graph-gap-count"
								data-testid="graph-gap-count"
								data-gap={gap.key}
								data-count={gap.count}
							>
								{gap.count} dependency lines share {gap.description}. Hover a line to inspect it.
							</p>
						{/each}
					</div>
				</Panel>
			{/if}
		</SvelteFlow>
		</div>

	{#if graph.unresolved.length > 0}

		<!-- Unresolved relations are content, not a notice: they take their own
		     row at the bottom of the pane so no node is ever covered. -->
		<UnresolvedList items={graph.unresolved} collapsible />
	{/if}
</div>
