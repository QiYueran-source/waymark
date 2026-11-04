<script lang="ts">
	import { onMount } from 'svelte';
	import PaneState from './PaneState.svelte';
	import ReadFailure from './ReadFailure.svelte';
	import Shape from './Shape.svelte';
	import type { Failure } from '$lib/failure';
	import { diagnosticText } from '$lib/api';
	import { shapeFor, stateLabel, statusLabel, type ItemDetail } from '$lib/types';

	let {
		id,
		detail,
		failure,
		loading,
		onClose,
		onSelect
	}: {
		id: string;
		detail: ItemDetail | null;
		failure: Failure | null;
		loading: boolean;
		onClose: () => void;
		onSelect: (id: string) => void;
	} = $props();

	let panel = $state<HTMLElement>();

	const stateClass = $derived(detail ? `state state-${detail.state}` : 'state');

	function relationships(item: ItemDetail): Array<{ label: string; ids: string[] }> {
		const parent = item.relationships.parent;
		return [
			{ label: 'Parent', ids: parent === null ? [] : [parent] },
			{ label: 'Children', ids: item.relationships.children },
			{ label: 'Dependencies', ids: item.relationships.dependencies },
			{ label: 'Dependents', ids: item.relationships.dependents },
			{ label: 'Soft links', ids: item.relationships.soft_links }
		];
	}

	const relationshipGroups = $derived(detail ? relationships(detail) : []);
	const customFieldEntries = $derived(detail ? Object.entries(detail.custom_fields) : []);

	/**
	 * Move focus into the panel when it opens.
	 *
	 * On open the trigger button still holds focus, so a plain "is focus inside?"
	 * check is not enough: the panel is the thing that was just announced.
	 */
	onMount(() => {
		panel?.focus();
	});

	/**
	 * When a read replaces the panel's content, the focused control inside it can
	 * be removed and focus falls back to the document body. Reclaim focus in that
	 * case only: focus the user has moved elsewhere (the tree) is left alone.
	 */
	$effect(() => {
		void loading;
		void failure;
		void detail;
		const element = panel;
		const active = document.activeElement;
		if (element && (active === null || active === document.body)) {
			element.focus();
		}
	});

	/**
	 * Escape closes the panel, but only from inside it.
	 *
	 * The listener is on the window because Escape must work from any control in
	 * the panel; the focus check keeps a key pressed in the tree out of it.
	 */
	function onKeydown(event: KeyboardEvent): void {
		if (event.key !== 'Escape') return;
		if (!panel?.contains(document.activeElement)) return;
		event.preventDefault();
		onClose();
	}
</script>

<svelte:window onkeydown={onKeydown} />

<aside
	class="detail"
	aria-labelledby="detail-title"
	data-testid="detail-panel"
	tabindex="-1"
	bind:this={panel}
>
	<div class="detail-header">
		<div class="detail-heading">
			{#if detail}
				<h2 id="detail-title" data-testid="detail-title">{detail.title}</h2>
			{:else}
				<h2 id="detail-title" data-testid="detail-title">{id}</h2>
			{/if}
			<p class="detail-id">{id}</p>
		</div>
		<button class="close" type="button" onclick={onClose}>Close</button>
	</div>

	{#if loading}
		<!-- Loading is a status, not one of the five blocking/empty states. -->
		<p class="status" role="status">Loading item…</p>
	{:else if failure}
		<PaneState testId="detail-failure-state" kind="detail-failure">
			<ReadFailure
				failure={failure}
				title="Could not load this item"
				level={3}
				testId="detail-failure"
				note="No detail is shown: the panel renders only a complete read."
			/>
		</PaneState>
	{:else if detail}
		<div class="detail-body">
			<dl class="facts">
				<dt>Category</dt>
				<dd>
					<Shape shape={shapeFor(detail.category)} label={`${detail.category.name} category`} />
					{detail.category.name}
				</dd>
				<dt>Status</dt>
				<dd data-testid="detail-status">{statusLabel(detail.status)}</dd>
				<dt>State</dt>
				<dd data-testid="detail-state"><span class={stateClass}>{stateLabel(detail.state)}</span></dd>
				{#if detail.state === 'blocked' && detail.blocked_reason}
					<dt>Blocked reason</dt>
					<dd data-testid="detail-blocked-reason">{detail.blocked_reason}</dd>
				{/if}
				<dt>Owner</dt>
				<dd data-testid="detail-owner">{detail.owner ?? 'None'}</dd>
				<dt>Creator</dt>
				<dd>{detail.creator ?? 'None'}</dd>
				{#if detail.archived}
					<dt>Archived</dt>
					<dd>Yes</dd>
				{/if}
			</dl>

			<section class="description" aria-labelledby="description-heading">
				<h3 id="description-heading">Description</h3>
				{#if detail.body_html}
					<!-- body_html is sanitized by the service; this is the only HTML injection. -->
					<div class="item-body" data-testid="detail-body">{@html detail.body_html}</div>
				{:else}
					<p class="muted">No description.</p>
				{/if}
			</section>

			<section class="custom-fields" aria-labelledby="custom-heading">
				<h3 id="custom-heading">Custom fields</h3>
				{#if customFieldEntries.length === 0}
					<p class="muted">None.</p>
				{:else}
					<dl class="facts">
						{#each customFieldEntries as [name, value] (name)}
							<dt data-testid="custom-field-name">{name}</dt>
							<dd>{diagnosticText(value)}</dd>
						{/each}
					</dl>
				{/if}
			</section>

			<section class="relationships" aria-labelledby="relationships-heading">
				<h3 id="relationships-heading">Relationships</h3>
				<dl class="facts">
					{#each relationshipGroups as group (group.label)}
						<dt>{group.label}</dt>
						<dd>
							{#if group.ids.length === 0}
								None
							{:else}
								<span class="id-list">
									{#each group.ids as relatedId (relatedId)}
										<button class="link" type="button" onclick={() => onSelect(relatedId)}>
											{relatedId}
										</button>
									{/each}
								</span>
							{/if}
						</dd>
					{/each}
				</dl>
			</section>
		</div>
	{/if}
</aside>
