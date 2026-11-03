<script lang="ts">
	import Shape from './Shape.svelte';
	import type { Failure } from '$lib/failure';
	import { diagnosticText } from '$lib/api';
	import {
		shapeFor,
		stateLabel,
		statusLabel,
		type ItemDetail,
		type WorkState
	} from '$lib/types';

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

	const stateClass = $derived(detail ? `state state-${detail.state}` : 'state');

	function fieldValue(value: unknown): string {
		if (typeof value === 'string') return value;
		if (value === null) return 'null';
		if (value === undefined) return 'undefined';
		try {
			return JSON.stringify(value);
		} catch {
			return String(value);
		}
	}

	function relationships(detail: ItemDetail): Array<{ label: string; ids: string[] }> {
		return [
			{ label: 'Children', ids: detail.relationships.children },
			{ label: 'Dependencies', ids: detail.relationships.dependencies },
			{ label: 'Dependents', ids: detail.relationships.dependents },
			{ label: 'Soft links', ids: detail.relationships.soft_links }
		];
	}

	const relationshipGroups = $derived(detail ? relationships(detail) : []);
	const customFieldEntries = $derived(detail ? Object.entries(detail.custom_fields) : []);
	const blockedState: WorkState = 'blocked';

	function onKeydown(event: KeyboardEvent): void {
		if (event.key === 'Escape') {
			event.preventDefault();
			onClose();
		}
	}
</script>

<svelte:window onkeydown={onKeydown} />

<aside class="detail" aria-labelledby="detail-title" data-testid="detail-panel">
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
		<p class="status" role="status">Loading item…</p>
	{:else if failure}
		<div class="failure" role="alert" data-testid="detail-failure">
			<h3>Could not load this item</h3>
			<p class="failure-code"><code>{failure.code}</code></p>
			<p class="failure-message">{failure.message}</p>
			{#if failure.diagnostics.length > 0}
				<ul class="diagnostics" aria-label="Diagnostics">
					{#each failure.diagnostics as diagnostic, index (index)}
						<li>{diagnosticText(diagnostic)}</li>
					{/each}
				</ul>
			{/if}
		</div>
	{:else if detail}
		<div class="detail-body">
			<dl class="facts">
				<dt>Category</dt>
				<dd>
					<Shape
						shape={shapeFor(detail.category)}
						label={`${detail.category.name} category`}
					/>
					{detail.category.name}
				</dd>
				<dt>Status</dt>
				<dd data-testid="detail-status">{statusLabel(detail.status)}</dd>
				<dt>State</dt>
				<dd data-testid="detail-state"><span class={stateClass}>{stateLabel(detail.state)}</span></dd>
				{#if detail.state === blockedState && detail.blocked_reason}
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
							<dd>{fieldValue(value)}</dd>
						{/each}
					</dl>
				{/if}
			</section>

			<section class="relationships" aria-labelledby="relationships-heading">
				<h3 id="relationships-heading">Relationships</h3>
				<dl class="facts">
					<dt>Parent</dt>
					<dd>
						{#if detail.relationships.parent}
							<button
								class="link"
								type="button"
								onclick={() => onSelect(detail.relationships.parent as string)}
							>
								{detail.relationships.parent}
							</button>
						{:else}
							None
						{/if}
					</dd>
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
