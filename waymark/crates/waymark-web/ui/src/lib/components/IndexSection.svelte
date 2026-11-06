<script lang="ts">
	/**
	 * The dashboard's active-item index: one page of rows with Previous/Next
	 * navigation driven by the payload.
	 *
	 * It loads page 1 exactly once on mount and re-fetches only when the reader
	 * presses Previous or Next: there is no polling, timer, or auto-refresh, by
	 * design. A transport failure degrades this section alone — the tree above
	 * still loads.
	 */
	import { onMount } from 'svelte';
	import ReadFailure from './ReadFailure.svelte';
	import { ApiFailure, fetchIndex } from '$lib/api';
	import { readDiagnostics } from '$lib/failure';
	import type { Failure } from '$lib/failure';
	import { freshnessLabel, statusLabel, type IndexData } from '$lib/types';

	let {
		fetchImpl = globalThis.fetch
	}: {
		/** Injected in tests; the page always uses the same-origin `fetch`. */
		fetchImpl?: typeof fetch;
	} = $props();

	/** The page currently shown; component-local by design. */
	let page = $state(1);
	let data = $state<IndexData | null>(null);
	let loading = $state(true);
	let failure = $state<Failure | null>(null);

	/** The rows are not a complete/current index: the banner must show. */
	const concerning = $derived(
		data !== null && (data.state.complete === false || data.state.current === false)
	);

	/** The report groups the state diagnostics like a read failure's do. */
	const report = $derived(
		data === null
			? null
			: readDiagnostics({
					code: 'INDEX_DIAGNOSTICS',
					message: 'index diagnostics',
					diagnostics: data.state.diagnostics
				})
	);

	async function loadIndex(next: number): Promise<void> {
		page = next;
		loading = true;
		failure = null;
		try {
			data = await fetchIndex(next, fetchImpl);
		} catch (error) {
			failure = toFailure(error);
		} finally {
			loading = false;
		}
	}

	function toFailure(error: unknown): Failure {
		if (error instanceof ApiFailure) {
			return { code: error.code, message: error.message, diagnostics: error.diagnostics };
		}
		return {
			code: 'UNEXPECTED_ERROR',
			message: error instanceof Error ? error.message : String(error),
			diagnostics: []
		};
	}

	onMount(() => {
		void loadIndex(1);
	});
</script>

<section class="index-section" aria-label="Active items">
	<h2>Active items</h2>

	{#if loading}
		<p class="status" role="status" data-testid="index-loading">Reading the active-item index…</p>
	{:else if failure}
		<!-- An index failure must not fail the page: the tree above still loads
		     and this block only explains why the index is missing. -->
		<ReadFailure
			failure={failure}
			title="Could not read the active-item index"
			level={3}
			testId="index-failure"
			note="The tree view still works; only the index section is missing."
		/>
	{:else if data}
		{#if concerning}
			<!-- The prominent banner: the state, the shortfall counts, and the
			     diagnostic summary. It reuses the notice palette and the notice
			     style so the page reads one language for "not current". -->
			<div class="notice index-banner" role="alert" data-testid="index-banner">
				<p class="index-banner-title">
					The rows are {freshnessLabel(data.state.freshness).toLowerCase()}; the index is
					{data.state.complete ? 'current' : 'incomplete'}
				</p>
				{#if data.state.unreadable_documents > 0 || data.state.uncertain_paths > 0}
					<p class="index-banner-counts" data-testid="index-banner-counts">
						{data.state.unreadable_documents} document(s) could not be read and
						{data.state.uncertain_paths} item-shaped path(s) are uncertain
					</p>
				{/if}
				{#if report && report.documents.length > 0}
					<p class="index-banner-diagnostics" data-testid="index-banner-diagnostics">
						{report.documents.length} document finding(s): {report.documents
							.map((problem) => problem.message)
							.join(' · ')}
					</p>
				{/if}
				{#if report && report.uncertain.length > 0}
					<p class="index-banner-diagnostics" data-testid="index-banner-uncertain">
						{report.uncertain.length} uncertain path(s): {report.uncertain
							.map((problem) => problem.message)
							.join(' · ')}
					</p>
				{/if}
				{#if data.state.notice}
					<p class="index-banner-notice" data-testid="index-banner-notice">
						{data.state.notice}
					</p>
				{/if}
			</div>
		{/if}

		{#if data.rows.length > 0}
			<table class="index-table" data-testid="index-table">
				<thead>
					<tr>
						<th scope="col">ID</th>
						<th scope="col">Title</th>
						<th scope="col">Category</th>
						<th scope="col">Status</th>
						<th scope="col">Path</th>
					</tr>
				</thead>
				<tbody>
					{#each data.rows as row (row.id)}
						<tr data-testid="index-row">
							<td><code>{row.id}</code></td>
							<td>{row.title}</td>
							<td>{row.category.name}</td>
							<td>{statusLabel(row.status)}</td>
							<td><code>{row.path}</code></td>
						</tr>
					{/each}
				</tbody>
			</table>
		{:else}
			<p class="status" data-testid="index-empty">
				No active work items are in this index.
			</p>
		{/if}

		<div class="index-nav">
			<button
				type="button"
				data-testid="index-prev"
				onclick={() => void loadIndex(page - 1)}
				disabled={!data.has_previous || loading}
			>
				Previous
			</button>
			<span class="index-page" data-testid="index-page">Page {data.page}</span>
			<button
				type="button"
				data-testid="index-next"
				onclick={() => void loadIndex(page + 1)}
				disabled={!data.has_next || loading}
			>
				Next
			</button>
		</div>
	{/if}
</section>
