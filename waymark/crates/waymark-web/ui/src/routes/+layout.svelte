<script lang="ts">
	// The palette and layout of the app; graph-library styles live with the
	// graph component, so this shell imports no graph library at all.
	import '../app.css';

	let { children } = $props();
</script>

{@render children()}
