<script lang="ts">
	import Shape from './Shape.svelte';
	import { shapeFor, type UnresolvedItem } from '$lib/types';

	let { items }: { items: UnresolvedItem[] } = $props();

	const REASONS: Record<UnresolvedItem['reason'], (parent: string) => string> = {
		parent_missing: (parent) => `its parent ${parent} is missing`,
		parent_archived: (parent) => `its parent ${parent} is archived`,
		parent_cycle: (parent) => `its parent ${parent} forms a cycle`
	};

	function explain(item: UnresolvedItem): string {
		return REASONS[item.reason]?.(item.parent) ?? `its parent relation is unresolved`;
	}
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
				<span class="unresolved-reason">{explain(item)}</span>
			</li>
		{/each}
	</ul>
</section>
