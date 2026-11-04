<script lang="ts">
	/**
	 * The grouped view's category scope.
	 *
	 * Options come from the payload's `categories` — every configured category,
	 * whether or not it holds active work — so a quiet category can still be
	 * chosen and then shows its empty state rather than vanishing from the
	 * control. Nothing here decides what filtering means: the selection is
	 * handed to `$lib/tree`, which stays the one place that knows.
	 */
	import Shape from './Shape.svelte';
	import { shapeFor } from '$lib/types';
	import type { Category } from '$lib/types';

	let {
		categories,
		selected,
		onToggle,
		onSelectAll,
		onClearAll
	}: {
		categories: Category[];
		selected: ReadonlySet<string>;
		onToggle: (key: string) => void;
		onSelectAll: () => void;
		onClearAll: () => void;
	} = $props();

	const allSelected = $derived(categories.every((category) => selected.has(category.key)));
	const noneSelected = $derived(categories.every((category) => !selected.has(category.key)));
</script>

<div class="category-filter" data-testid="category-filter">
	<div class="filter-head">
		<span class="filter-label" id="category-filter-label">Categories</span>
		<button
			type="button"
			aria-label="Select all categories"
			onclick={onSelectAll}
			disabled={allSelected}
		>
			Select all
		</button>
		<button
			type="button"
			aria-label="Clear all categories"
			onclick={onClearAll}
			disabled={noneSelected}
		>
			Clear all
		</button>
	</div>

	<div class="filter-options" role="group" aria-labelledby="category-filter-label">
		{#each categories as category (category.key)}
			{@const isSelected = selected.has(category.key)}
			<button
				type="button"
				class="filter-option"
				class:selected={isSelected}
				aria-pressed={isSelected}
				aria-label={`${category.name}, ${isSelected ? 'selected' : 'not selected'}`}
				data-testid={`category-option-${category.key}`}
				onclick={() => onToggle(category.key)}
			>
				<Shape shape={shapeFor(category)} label={`${category.name} category`} size={12} />
				<span class="filter-option-name">{category.name}</span>
			</button>
		{/each}
	</div>
</div>
