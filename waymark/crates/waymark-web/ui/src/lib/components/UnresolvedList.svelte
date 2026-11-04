<script module lang="ts">
	// Two hosts render this list (the tree panel and the graph drawer), so its
	// heading and content ids must be unique per instance. The SPA renders on
	// the client only (`ssr = false`), so a module counter is deterministic
	// enough for hydration-free ids.
	let instances = 0;
</script>

<script lang="ts">
	import Shape from './Shape.svelte';
	import { shapeFor, unresolvedReason, type UnresolvedItem } from '$lib/types';

	let {
		items,
		collapsible = false
	}: {
		items: UnresolvedItem[];
		/**
		 * Render as the graph's bottom drawer: a real button header with the
		 * entry count, folding the same content the tree shows in its full
		 * panel. The drawer is open by default so the reasons are visible
		 * without a click.
		 */
		collapsible?: boolean;
	} = $props();

	const instance = ++instances;
	const headingId = `unresolved-heading-${instance}`;
	const contentId = `unresolved-content-${instance}`;

	let open = $state(true);
</script>

<section
	class="unresolved"
	class:unresolved-drawer={collapsible}
	aria-labelledby={headingId}
	data-testid="unresolved"
>
	{#if collapsible}
		<h2 id={headingId} class="unresolved-heading">
			<button
				type="button"
				class="unresolved-toggle"
				data-testid="unresolved-toggle"
				aria-expanded={open}
				aria-controls={contentId}
				onclick={() => (open = !open)}
			>
				<span class="unresolved-toggle-mark" aria-hidden="true">{open ? '▾' : '▸'}</span>
				Unresolved relations ({items.length})
			</button>
		</h2>
	{:else}
		<h2 id={headingId}>Unresolved relations</h2>
	{/if}
	<div id={contentId} class="unresolved-content" hidden={collapsible && !open}>
		<p class="explanation">
			These items exist but are not placed in the tree, because their parent cannot be reached.
			They are listed here instead of being invented as roots.
		</p>
		<ul>
			{#each items as item (item.id)}
				<li data-testid="unresolved-item" data-reason={item.reason}>
					<span class="category" title={item.category.name}>
						<Shape shape={shapeFor(item.category)} label={`${item.category.name} category`} />
					</span>
					<span class="unresolved-title">{item.title}</span>
					<span class="edge-id">{item.id}</span>
					<span class="unresolved-reason">{unresolvedReason(item)}</span>
				</li>
			{/each}
		</ul>
	</div>
</section>
