<script lang="ts">
	// The graph's structural styles come from the drawing library; the palette
	// and the card styling stay in app.css.
	import '@xyflow/svelte/dist/base.css';
	import '../app.css';

	let { children } = $props();
</script>

{@render children()}
