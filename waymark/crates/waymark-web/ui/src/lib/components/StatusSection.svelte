<script lang="ts">
	/**
	 * The dashboard's status section: the projection's completeness and
	 * freshness, the diagnostic summary, and the derived-cache state.
	 *
	 * It is loaded exactly once per page load and never refreshes itself: there
	 * is no in-page refresh control, polling, watcher, or timer, by design.
	 */
	import ReadFailure from './ReadFailure.svelte';
	import { readDiagnostics } from '$lib/failure';
	import type { Failure } from '$lib/failure';
	import { completenessLabel, freshnessLabel, type StatusData } from '$lib/types';

	let {
		status,
		loading,
		failure
	}: {
		/** The status payload, `null` while it loads. */
		status: StatusData | null;
		/** Whether the status read is in flight. */
		loading: boolean;
		/** A transport failure of the status read; the section degrades, not the page. */
		failure: Failure | null;
	} = $props();

	/** The projection is incomplete or not current: the banner must show. */
	const concerning = $derived(
		status !== null &&
			(status.projection.completeness !== 'complete' ||
				status.projection.freshness !== 'fresh')
	);

	/** The report groups the status diagnostics like a read failure's do. */
	const report = $derived(
		status === null
			? null
			: readDiagnostics({
					code: 'STATUS_DIAGNOSTICS',
					message: 'projection diagnostics',
					diagnostics: status.projection.diagnostics
				})
	);
</script>

<section class="status-section" aria-label="Cache and projection status">
	<h2>Cache and projection status</h2>

	{#if loading}
		<p class="status" role="status" data-testid="status-loading">Reading service status…</p>
	{:else if failure}
		<!-- A status failure must not fail the page: the tree above still loads
		     and this block only explains why the status is missing. -->
		<ReadFailure
			failure={failure}
			title="Could not read the service status"
			level={3}
			testId="status-failure"
			note="The tree view still works; only the status section is missing."
		/>
	{:else if status}
		{#if concerning}
			<!-- The prominent banner: the state, the shortfall counts, and the
			     diagnostic summary. Never silence the tree below it: this is the
			     one place the dashboard states that the projection is not
			     complete/current. -->
			<div class="notice status-banner" role="alert" data-testid="status-banner">
				<p class="status-banner-title">
					The projection is {freshnessLabel(status.projection.freshness).toLowerCase()} and
					{completenessLabel(status.projection.completeness).toLowerCase()}
				</p>
				{#if status.projection.unreadable_documents > 0 || status.projection.uncertain_paths > 0}
					<p class="status-banner-counts" data-testid="status-banner-counts">
						{status.projection.unreadable_documents} document(s) could not be read and
						{status.projection.uncertain_paths} item-shaped path(s) are uncertain
					</p>
				{/if}
				{#if report && report.documents.length > 0}
					<p class="status-banner-diagnostics" data-testid="status-banner-diagnostics">
						{report.documents.length} document finding(s): {report.documents
							.map((problem) => problem.message)
							.join(' · ')}
					</p>
				{/if}
				{#if report && report.uncertain.length > 0}
					<p class="status-banner-diagnostics" data-testid="status-banner-uncertain">
						{report.uncertain.length} uncertain path(s): {report.uncertain
							.map((problem) => problem.message)
							.join(' · ')}
					</p>
				{/if}
			</div>
		{/if}

		<dl class="status-facts" data-testid="status-facts">
			<div>
				<dt>Completeness</dt>
				<dd data-testid="status-completeness">{completenessLabel(status.projection.completeness)}</dd>
			</div>
			<div>
				<dt>Freshness</dt>
				<dd data-testid="status-freshness">{freshnessLabel(status.projection.freshness)}</dd>
			</div>
			<div>
				<dt>Unreadable documents</dt>
				<dd data-testid="status-unreadable">{status.projection.unreadable_documents}</dd>
			</div>
			<div>
				<dt>Uncertain paths</dt>
				<dd data-testid="status-uncertain">{status.projection.uncertain_paths}</dd>
			</div>
			<div>
				<dt>Last successful load</dt>
				<dd data-testid="status-last-success">
					{status.projection.last_success ?? 'never loaded'}
				</dd>
			</div>
			<div>
				<dt>Cache</dt>
				<dd data-testid="status-cache-enabled">
					{status.cache.enabled ? 'enabled' : 'disabled'}
				</dd>
			</div>
			<div>
				<dt>Cache capacity</dt>
				<dd data-testid="status-cache-capacity">{status.cache.capacity}</dd>
			</div>
			<div>
				<dt>Cache entries</dt>
				<dd data-testid="status-cache-total">{status.cache.total}</dd>
			</div>
			<div>
				<dt>Body / detail / query entries</dt>
				<dd data-testid="status-cache-by-class">{status.cache.by_class.body} / {status.cache.by_class.detail} / {status.cache.by_class.query}</dd>
			</div>
			<div>
				<dt>Hits / misses / evictions</dt>
				<dd data-testid="status-counters">{status.counters.hits} / {status.counters.misses} / {status.counters.evictions}</dd>
			</div>
		</dl>
	{/if}
</section>
