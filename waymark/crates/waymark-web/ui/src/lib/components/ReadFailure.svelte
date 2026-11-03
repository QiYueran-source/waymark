<script lang="ts">
	import { diagnosticText } from '$lib/api';
	import type { Failure } from '$lib/failure';

	/**
	 * The one read-failure block: code, message, and diagnostics.
	 *
	 * It is shared by the tree read and the item read, so the heading, test ids,
	 * and the closing note are parameters. The heading level follows where the
	 * block sits: the detail panel already has an `h2` title.
	 */
	let {
		failure,
		title = 'Could not read the project',
		level = 2,
		testId = 'read-failure',
		note = 'No partial tree is shown: the tree is rendered only from a complete read.'
	}: {
		failure: Failure;
		title?: string;
		level?: 2 | 3;
		testId?: string;
		/** The closing explanation; `null` omits it where it would not be true. */
		note?: string | null;
	} = $props();
</script>

<section class="failure read-failure" role="alert" data-testid={testId}>
	<svelte:element this={level === 2 ? 'h2' : 'h3'} class="failure-title">{title}</svelte:element>
	<p class="failure-code" data-testid={`${testId}-code`}><code>{failure.code}</code></p>
	<p class="failure-message" data-testid={`${testId}-message`}>{failure.message}</p>
	{#if failure.diagnostics.length > 0}
		<h3>Diagnostics</h3>
		<ul class="diagnostics" data-testid={`${testId}-diagnostics`}>
			{#each failure.diagnostics as diagnostic, index (index)}
				<li>{diagnosticText(diagnostic)}</li>
			{/each}
		</ul>
	{:else}
		<p class="muted">The service reported no further diagnostics.</p>
	{/if}
	{#if note}
		<p class="muted">{note}</p>
	{/if}
</section>
