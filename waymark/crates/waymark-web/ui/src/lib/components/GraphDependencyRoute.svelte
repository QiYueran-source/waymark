<script lang="ts">
	/**
	 * A dependency drawn along a route of its own.
	 *
	 * The route is decided by the layout and handed over as `data`: this
	 * component paints the path it is given and measures nothing, so the drawing
	 * stays a function of the model alone. It is the one place outside
	 * `GraphView.svelte` that imports the drawing library, because a custom edge
	 * has to be a component; it reuses the library's `BaseEdge`, so the style,
	 * the arrow marker, the accessible name and the label portal are the same as
	 * for every other edge.
	 */
	import { BaseEdge } from '@xyflow/svelte';
	import type { EdgeProps } from '@xyflow/svelte';

	import type { DependencyRoute } from '$lib/graph';

	let { id, data, style, markerEnd, label, labelStyle }: EdgeProps = $props();

	const route = $derived((data as { route?: DependencyRoute } | undefined)?.route);
</script>

{#if route}
	<BaseEdge
		{id}
		path={route.path}
		labelX={route.labelX}
		labelY={route.labelY}
		{style}
		{markerEnd}
		{label}
		{labelStyle}
	/>
{/if}
