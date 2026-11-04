<script lang="ts">
	import { diagnosticText } from '$lib/api';
	import { readDiagnostics, type LocatedProblem } from '$lib/failure';
	import type { Failure } from '$lib/failure';

	/**
	 * The one read-failure block: code, message, and diagnostics.
	 *
	 * It is shared by the tree read and the item read, so the heading, test ids,
	 * and the closing note are parameters. The heading level follows where the
	 * block sits: the detail panel already has an `h2` title.
	 *
	 * Diagnostics arrive as `Finding` values. The ones that name a document or an
	 * inventory path are grouped by that cause and shown one per line, because
	 * that is the part a reader acts on; anything unrecognised is still shown
	 * verbatim rather than dropped.
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

	const report = $derived(readDiagnostics(failure));
</script>

{#snippet problems(list: LocatedProblem[], id: string)}
	<ul class="problems" data-testid={id}>
		{#each list as problem, index (index)}
			<li class="problem">
				<span class="problem-place">
					<code class="problem-code">{problem.code}</code>
					{#if problem.where}
						<span class="problem-where">{problem.where}</span>
					{/if}
					{#if problem.field}
						<span class="problem-field">field <code>{problem.field}</code></span>
					{/if}
				</span>
				<span class="problem-message">{problem.message}</span>
			</li>
		{/each}
	</ul>
{/snippet}

<section class="failure read-failure" role="alert" data-testid={testId}>
	<svelte:element this={level === 2 ? 'h2' : 'h3'} class="failure-title">{title}</svelte:element>
	<p class="failure-code" data-testid={`${testId}-code`}><code>{failure.code}</code></p>
	<p class="failure-message" data-testid={`${testId}-message`}>{failure.message}</p>
	{#if report.documents.length > 0}
		<h3>Documents the projection could not use</h3>
		{@render problems(report.documents, `${testId}-documents`)}
	{/if}
	{#if report.uncertain.length > 0}
		<h3>Paths the inventory is uncertain about</h3>
		{@render problems(report.uncertain, `${testId}-uncertain`)}
	{/if}
	{#if report.other.length > 0}
		<h3>Other findings</h3>
		{@render problems(report.other, `${testId}-other`)}
	{/if}
	{#if report.leftovers.length > 0}
		<h3>Diagnostics</h3>
		<ul class="diagnostics" data-testid={`${testId}-diagnostics`}>
			{#each report.leftovers as diagnostic, index (index)}
				<li>{diagnosticText(diagnostic)}</li>
			{/each}
		</ul>
	{:else if failure.diagnostics.length === 0}
		<p class="muted">The service reported no further diagnostics.</p>
	{/if}
	{#if report.blocked}
		<p class="failure-guidance" data-testid={`${testId}-guidance`}>
			This view is read-only, so it cannot repair project documents. Run <code>waymark validate</code
			> in the project to diagnose them.
		</p>
	{/if}
	{#if note}
		<p class="muted">{note}</p>
	{/if}
</section>
