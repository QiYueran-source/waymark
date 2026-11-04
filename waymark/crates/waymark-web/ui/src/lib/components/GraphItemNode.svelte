<script lang="ts">
	/**
	 * The rendering library's node slot.
	 *
	 * The library hands a node component the node's `data` payload, so this
	 * adapter unpacks it and renders the app's own card. Keeping the unpacking
	 * here means `GraphNode.svelte` stays a plain component that takes
	 * work-item values and knows nothing about the drawing library.
	 */
	import GraphNode from './GraphNode.svelte';
	import type { GraphNodePayload } from '$lib/graph';

	let { data }: { data: GraphNodePayload } = $props();
</script>

<GraphNode
	node={data.node}
	selected={data.selected}
	graphY={data.graphY}
	onSelect={data.onSelect}
/>
