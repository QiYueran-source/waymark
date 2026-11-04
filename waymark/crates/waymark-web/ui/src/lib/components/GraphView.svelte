<script lang="ts">
	/**
	 * The graph presentation: the same active tree, drawn top to bottom.
	 *
	 * This is the only module under `ui/src` that imports the drawing library or
	 * the layout engine, so both stay replaceable and the rest of the app keeps
	 * dealing in `WorkGraph` values. Placement is computed from the model — one
	 * row per generation, a parent's row above its children's — and handed to
	 * the renderer as fixed geometry. Nothing here measures the DOM to decide
	 * where a node goes, so the same input always produces the same drawing.
	 *
	 * The view is read-only: no drag, no connect, no delete, and no selection
	 * state of the renderer's own. The only interactive element is the app-owned
	 * title button inside each card.
	 */
	import { Graph as DagreGraph, layout as dagreLayout } from '@dagrejs/dagre';
	import { Panel, Position, SvelteFlow } from '@xyflow/svelte';
	import type { Edge, Node, NodeTypes } from '@xyflow/svelte';

	import GraphItemNode from './GraphItemNode.svelte';
	import { edgeLabel, graphNodeLabel, type GraphNodePayload, type WorkGraph } from '$lib/graph';

	let {
		graph,
		selectedId = null,
		onSelect
	}: {
		graph: WorkGraph;
		/** The item whose detail panel is open, if any. */
		selectedId?: string | null;
		/** Selecting a node; the trigger lets focus return after the panel closes. */
		onSelect: (id: string, trigger?: HTMLElement) => void;
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
	 * become edges.
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
					onSelect
				},
				x: (center?.x ?? 0) - NODE_WIDTH / 2,
				y
			};
		});
	}

	const placed = $derived(placeNodes(graph));

	/**
	 * The nodes handed to the renderer.
	 *
	 * `draggable`/`selectable`/`connectable`/`focusable` are all off: the graph
	 * is a drawing, and the only tab stop per node is the card's own button.
	 * The static handles let the renderer anchor the solid edges without
	 * measuring anything.
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
				{ type: 'source', position: Position.Bottom, x: NODE_WIDTH / 2, y: NODE_HEIGHT },
				{ type: 'target', position: Position.Top, x: NODE_WIDTH / 2, y: 0 }
			],
			domAttributes: {
				'data-node-id': payload.node.id,
				'data-depth': String(payload.node.depth),
				'data-parent-id': payload.node.parentId ?? ''
			}
		}))
	);

	/** One solid edge per parent-child relation, named for assistive technology. */
	const edges: Edge[] = $derived(
		graph.edges.map((edge) => ({
			id: edge.id,
			source: edge.from,
			target: edge.to,
			ariaLabel: edgeLabel(graph, edge),
			domAttributes: {
				'data-testid': 'graph-edge',
				'data-from': edge.from,
				'data-to': edge.to,
				'data-edge-kind': 'parent-child'
			}
		}))
	);

	const nodeTypes: NodeTypes = { workitem: GraphItemNode };
</script>

<div class="graph-pane" data-testid="graph">
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
		proOptions={{ hideAttribution: true }}
	>
		<Panel position="top-left">
			<div class="graph-legend" data-testid="graph-legend">
				<svg class="legend-edge" viewBox="0 0 24 8" aria-hidden="true" focusable="false">
					<line x1="1" y1="4" x2="23" y2="4" />
				</svg>
				<span>Solid line: parent-child</span>
			</div>
		</Panel>
	</SvelteFlow>
</div>
