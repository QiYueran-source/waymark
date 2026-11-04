<script lang="ts">
	import Shape from './Shape.svelte';
	import { shapeFor, unresolvedReason, type UnresolvedItem } from '$lib/types';

	let { items }: { items: UnresolvedItem[] } = $props();
</script>

<section class="unresolved" aria-labelledby="unresolved-heading" data-testid="unresolved">
	<h2 id="unresolved-heading">Unresolved relations</h2>
	<p class="explanation">
		These items exist but are not placed in the tree, because their parent cannot be reached. They
		are listed here instead of being invented as roots.
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
</section>
