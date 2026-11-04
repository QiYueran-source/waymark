<script lang="ts">
	/**
	 * The viewport bridge: the one place inside the drawing that can hand the
	 * canvas controls to the app.
	 *
	 * The drawing library exposes its pan/zoom API only to components rendered
	 * below it, so this adapter — which draws nothing — receives that API and
	 * passes it up. `GraphView.svelte` then drives zoom, pan, fit, and reset
	 * without itself importing the library's hook.
	 */
	import { useSvelteFlow } from '@xyflow/svelte';

	import type { GraphFlow } from '$lib/graph';

	let { onReady }: { onReady: (flow: GraphFlow) => void } = $props();

	const flow = useSvelteFlow();

	$effect(() => {
		onReady({
			setViewport: (viewport) => flow.setViewport(viewport),
			getViewport: () => flow.getViewport(),
			zoomIn: () => flow.zoomIn(),
			zoomOut: () => flow.zoomOut()
		});
	});
</script>
