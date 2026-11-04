<script lang="ts">
	/**
	 * The graph presentation: the same active tree, drawn top to bottom.
	 *
	 * This is the only module under `ui/src` that imports the drawing library's
	 * components and the layout engine; its child `GraphBridge.svelte` imports
	 * the library's `useSvelteFlow` hook to hand the pan/zoom API up. Both stay
	 * inside this layer, no dagre or library type escapes it, and the rest of
	 * the app keeps dealing in `WorkGraph` values. The library stylesheet is
	 * imported here too, so the app shell imports no graph library at all.
	 * Placement is computed from the model — one
	 * row per generation, a parent's row above its children's — and handed to
	 * the renderer as fixed geometry. Nothing here measures the DOM to decide
	 * where a node goes, so the same input always produces the same drawing.
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

	import GraphBridge from './GraphBridge.svelte';
	import GraphItemNode from './GraphItemNode.svelte';
	import {
		GRAPH_NOMINAL_CANVAS,
		MAX_GRAPH_ZOOM,
		MIN_FIT_ZOOM,
		MIN_GRAPH_ZOOM,
		edgeLabel,
		fittedViewport,
		graphBounds,
		graphNodeLabel,
		sameViewport,
		viewportContains,
		type GraphDependencyEdge,
		type GraphFlow,
		type GraphNodePayload,
		type GraphViewport,
		type WorkGraph
	} from '$lib/graph';

	let {
		graph,
		selectedId = null,
		onSelect,
		onToggle,
		restoredViewport = null,
		onViewportChange,
		onViewportReset
	}: {
		/**
		 * The drawing to render. Whether the dependency overlay is included is
		 * the model's decision (`buildGraph`'s `dependenciesVisible`), so the view
		 * renders `graph.dependencies` as given.
		 */
		graph: WorkGraph;
		/** The item whose detail panel is open, if any. */
		selectedId?: string | null;
		/** Selecting a node; the trigger lets focus return after the panel closes. */
		onSelect: (id: string, trigger?: HTMLElement) => void;
		/** Collapsing or expanding a node; the item itself is never changed. */
		onToggle: (id: string) => void;
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
	 * One fixed card size. The layout is computed from these numbers, so a node
	 * never has to be measured before its edges can be drawn.
	 */
	const NODE_WIDTH = 216;
	const NODE_HEIGHT = 84;
	const ROW_GAP = 56;
	const COLUMN_GAP = 28;
	const MARGIN = 16;

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
	 * The overlay's arrowhead, defined by the renderer's marker machinery and
	 * referenced from the edge path's target end. It is the one app-owned hook
	 * that makes the direction visible at a glance.
	 */
	const DEPENDENCY_MARKER: EdgeMarker = {
		type: MarkerType.ArrowClosed,
		color: 'var(--dash)',
		width: 14,
		height: 14
	};

	/** Dashed, muted, and never the hierarchy's solid stroke. */
	const DEPENDENCY_STYLE = 'stroke-dasharray: 6 4; stroke: var(--dash)';

	interface PlacedNode {
		payload: GraphNodePayload;
		x: number;
		y: number;
	}

	/**
	 * Lay the hierarchy out top to bottom.
	 *
	 * Every root of the walk starts its own tree; the engine puts each node
	 * below its parent, so depth and vertical position agree by construction.
	 * Work-item ids are passed through unchanged — the graph never renames, nor
	 * wraps an item in a synthetic node — and only solid parent-child relations
	 * become layout edges. The dashed overlay is deliberately absent here.
	 */
	function placeNodes(model: WorkGraph): PlacedNode[] {
		if (model.nodes.length === 0) return [];

		const layoutGraph = new DagreGraph();
		layoutGraph.setGraph({
			rankdir: 'TB',
			nodesep: COLUMN_GAP,
			ranksep: ROW_GAP,
			marginx: MARGIN,
			marginy: MARGIN
		});
		// The engine reads an edge label for every edge it creates, so it needs a
		// factory; without one it throws before laying anything out.
		layoutGraph.setDefaultEdgeLabel(() => ({}));

		for (const node of model.nodes) {
			layoutGraph.setNode(node.id, { width: NODE_WIDTH, height: NODE_HEIGHT });
		}
		for (const edge of model.edges) {
			layoutGraph.setEdge(edge.from, edge.to);
		}

		dagreLayout(layoutGraph);

		return model.nodes.map((node) => {
			const center = layoutGraph.node(node.id) as { x: number; y: number } | undefined;
			const y = (center?.y ?? 0) - NODE_HEIGHT / 2;
			return {
				payload: {
					node,
					graphY: y,
					selected: node.id === selectedId,
					collapsed: node.collapsed === true,
					onSelect,
					onToggle
				},
				x: (center?.x ?? 0) - NODE_WIDTH / 2,
				y
			};
		});
	}

	const placed = $derived(placeNodes(graph));

	/** Where each node sits, so an overlay can leave by the nearest side. */
	const placedById = $derived(
		new Map(placed.map((entry) => [entry.payload.node.id, { x: entry.x, y: entry.y }]))
	);

	/**
	 * Which pair of handles a dependency edge should use.
	 *
	 * The hierarchy is top-to-bottom, so most dependencies point up or down and
	 * the vertical pair is right. Two items on the same row — two roots, or a
	 * dependency between siblings — are drawn side by side instead, so the arrow
	 * arcs across rather than looping around.
	 */
	function dependencyHandles(from: string, to: string): { source: string; target: string } {
		const source = placedById.get(from);
		const target = placedById.get(to);
		if (source === undefined || target === undefined) {
			return { source: OUT_BOTTOM, target: IN_TOP };
		}
		const dy = target.y - source.y;
		if (Math.abs(dy) < 1) {
			return target.x >= source.x
				? { source: OUT_RIGHT, target: IN_LEFT }
				: { source: OUT_LEFT, target: IN_RIGHT };
		}
		return dy < 0
			? { source: OUT_TOP, target: IN_BOTTOM }
			: { source: OUT_BOTTOM, target: IN_TOP };
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
		placed.map(({ payload, x, y }) => ({
			id: payload.node.id,
			type: 'workitem',
			position: { x, y },
			width: NODE_WIDTH,
			height: NODE_HEIGHT,
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
					y: NODE_HEIGHT
				},
				{ id: IN_TOP, type: 'target', position: Position.Top, x: NODE_WIDTH / 2, y: 0 },
				{ id: OUT_TOP, type: 'source', position: Position.Top, x: NODE_WIDTH / 2, y: 0 },
				{
					id: IN_BOTTOM,
					type: 'target',
					position: Position.Bottom,
					x: NODE_WIDTH / 2,
					y: NODE_HEIGHT
				},
				{
					id: OUT_RIGHT,
					type: 'source',
					position: Position.Right,
					x: NODE_WIDTH,
					y: NODE_HEIGHT / 2
				},
				{ id: IN_LEFT, type: 'target', position: Position.Left, x: 0, y: NODE_HEIGHT / 2 },
				{ id: OUT_LEFT, type: 'source', position: Position.Left, x: 0, y: NODE_HEIGHT / 2 },
				{
					id: IN_RIGHT,
					type: 'target',
					position: Position.Right,
					x: NODE_WIDTH,
					y: NODE_HEIGHT / 2
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
		graph.edges.map((edge) => ({
			id: edge.id,
			source: edge.from,
			target: edge.to,
			sourceHandle: OUT_BOTTOM,
			targetHandle: IN_TOP,
			ariaLabel: edgeLabel(graph, edge),
			domAttributes: {
				'data-testid': 'graph-edge',
				'data-from': edge.from,
				'data-to': edge.to,
				'data-edge-kind': 'parent-child'
			}
		}))
	);

	/**
	 * One dashed, directed edge per dependency, arrow pointing at the item
	 * depended on. `data-arrow="target"` is the app-owned hook that states the
	 * direction without geometry; the accessible name says it in words too.
	 */
	function dependencyEdge(edge: GraphDependencyEdge): Edge {
		const handles = dependencyHandles(edge.from, edge.to);
		return {
			id: edge.id,
			source: edge.from,
			target: edge.to,
			sourceHandle: handles.source,
			targetHandle: handles.target,
			style: DEPENDENCY_STYLE,
			markerEnd: DEPENDENCY_MARKER,
			ariaLabel: edgeLabel(graph, edge),
			domAttributes: {
				'data-testid': 'graph-edge',
				'data-from': edge.from,
				'data-to': edge.to,
				'data-edge-kind': 'dependency',
				'data-arrow': 'target'
			}
		};
	}

	const edges: Edge[] = $derived([...hierarchyEdges, ...graph.dependencies.map(dependencyEdge)]);

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

	/** Where the whole visible drawing sits, in drawing coordinates. */
	const bounds = $derived(
		graphBounds(placed.map(({ x, y }) => ({ x, y, width: NODE_WIDTH, height: NODE_HEIGHT })))
	);

	/** The viewport that shows the whole visible drawing, centred. */
	const fitted = $derived(
		fittedViewport(bounds, canvasWidth(canvasBox), canvasHeight(canvasBox))
	);

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
			viewportContains(shown, bounds, canvasWidth(canvasBox), canvasHeight(canvasBox))
	);

	/** How wide the drawing's box is, or the nominal canvas if it has no size. */
	function canvasWidth(box: HTMLElement | undefined): number {
		const width = box?.clientWidth ?? 0;
		return width > 0 ? width : GRAPH_NOMINAL_CANVAS.width;
	}

	function canvasHeight(box: HTMLElement | undefined): number {
		const height = box?.clientHeight ?? 0;
		return height > 0 ? height : GRAPH_NOMINAL_CANVAS.height;
	}

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
		initial = restoredViewport ?? fitted;
		apply(initial);
	}

	// The drawing hands over its pan/zoom API through `GraphBridge`, and reports
	// `oninit` once that API is ready; either signal is enough to enter.
	$effect(() => {
		if (flow !== null) enterViewport();
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

<div class="graph-pane" data-testid="graph" bind:this={canvasBox}>
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
		proOptions={{ hideAttribution: true }}
	>
		<GraphBridge onReady={(handle) => (flow = handle)} />

		<Panel position="top-right">
			<div class="graph-controls" data-testid="graph-controls">
				<!-- Pan and zoom are the canvas's own gestures: dragging the
				     background, the wheel, a trackpad pinch, or two fingers on a
				     touch screen. These controls only repeat or undo them. -->
				<div class="graph-controls-row" role="group" aria-label="Canvas">
					<button type="button" data-testid="graph-zoom-out" onclick={zoomOut}>Zoom out</button>
					<button type="button" data-testid="graph-zoom-in" onclick={zoomIn}>Zoom in</button>
					<button type="button" data-testid="graph-fit" onclick={fitToView}>Fit to view</button>
					<button type="button" data-testid="graph-reset" onclick={resetViewport}>
						Reset viewport
					</button>
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

		<Panel position="top-left">
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
			</div>
		</Panel>
	</SvelteFlow>
</div>
