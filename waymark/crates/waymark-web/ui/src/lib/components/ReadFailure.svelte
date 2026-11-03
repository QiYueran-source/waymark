<script lang="ts">
	import { diagnosticText } from '$lib/api';
	import type { Failure } from '$lib/failure';

	let { failure }: { failure: Failure } = $props();
</script>

<section class="failure read-failure" role="alert" data-testid="read-failure">
	<h2>Could not read the project</h2>
	<p class="failure-code" data-testid="read-failure-code"><code>{failure.code}</code></p>
	<p class="failure-message" data-testid="read-failure-message">{failure.message}</p>
	{#if failure.diagnostics.length > 0}
		<h3>Diagnostics</h3>
		<ul class="diagnostics" data-testid="read-failure-diagnostics">
			{#each failure.diagnostics as diagnostic, index (index)}
				<li>{diagnosticText(diagnostic)}</li>
			{/each}
		</ul>
	{:else}
		<p class="muted">The service reported no further diagnostics.</p>
	{/if}
	<p class="muted">No partial tree is shown: the tree is rendered only from a complete read.</p>
</section>
