<script lang="ts">
	/**
	 * The grouped view's category scope, as one compact control.
	 *
	 * Options come from the payload's `categories` — every configured category,
	 * whether or not it holds active work — so a quiet category can still be
	 * chosen and then shows its empty state rather than vanishing from the
	 * control. Nothing here decides what filtering means: the selection is
	 * handed to `$lib/tree`, which stays the one place that knows.
	 *
	 * The popover is rendered only while open, so a closed control leaves no
	 * options in the accessibility tree and cannot cover the drawing. Escape
	 * closes it and returns focus to the trigger; an outside pointer press
	 * closes it without stealing focus.
	 */
	import { tick } from 'svelte';
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

	let open = $state(false);
	let query = $state('');
	let wrapper: HTMLElement | undefined = $state();
	let trigger: HTMLButtonElement | undefined = $state();
	let search: HTMLInputElement | undefined = $state();

	const selectedCount = $derived(categories.filter((category) => selected.has(category.key)).length);
	const allSelected = $derived(categories.every((category) => selected.has(category.key)));
	const noneSelected = $derived(categories.every((category) => !selected.has(category.key)));
	/** Searches the displayed name only; the state is unchanged by a search. */
	const filtered = $derived(
		query.trim() === ''
			? categories
			: categories.filter((category) =>
					category.name.toLowerCase().includes(query.trim().toLowerCase())
				)
	);

	function openPopover(): void {
		open = true;
		void tick().then(() => search?.focus());
	}

	function closePopover(restoreFocus = false): void {
		if (!open) return;
		open = false;
		query = '';
		if (restoreFocus) void tick().then(() => trigger?.focus());
	}

	function togglePopover(): void {
		if (open) closePopover(true);
		else openPopover();
	}

	/**
	 * Close on an outside pointer press. Events inside the control — including
	 * the trigger and the popover itself — are the reader's use of it.
	 */
	function handleOutside(event: Event): void {
		const target = event.target;
		if (!(target instanceof Node)) return;
		if (wrapper?.contains(target)) return;
		closePopover();
	}

	function handleKeydown(event: KeyboardEvent): void {
		if (!open || event.key !== 'Escape') return;
		event.preventDefault();
		event.stopPropagation();
		closePopover(true);
	}

	// While open, listen for the ways to dismiss the popover. The pointer events
	// cover mouse and touch; the click covers environments that dispatch only a
	// click, and an inside click is always ignored above.
	$effect(() => {
		if (!open) return;
		window.addEventListener('keydown', handleKeydown);
		document.addEventListener('pointerdown', handleOutside, true);
		document.addEventListener('mousedown', handleOutside, true);
		document.addEventListener('click', handleOutside, true);
		return () => {
			window.removeEventListener('keydown', handleKeydown);
			document.removeEventListener('pointerdown', handleOutside, true);
			document.removeEventListener('mousedown', handleOutside, true);
			document.removeEventListener('click', handleOutside, true);
		};
	});
</script>

<div class="category-filter" data-testid="category-filter" bind:this={wrapper}>
	<button
		type="button"
		class="category-trigger"
		data-testid="category-trigger"
		aria-haspopup="dialog"
		aria-expanded={open}
		aria-controls={open ? 'category-popover' : undefined}
		bind:this={trigger}
		onclick={togglePopover}
	>
		<span>Categories ({selectedCount})</span>
		<svg class="category-caret" viewBox="0 0 10 6" aria-hidden="true" focusable="false">
			<path d="M1 1l4 4 4-4" fill="none" stroke="currentColor" stroke-width="1.5" />
		</svg>
	</button>

	{#if open}
		<div
			class="category-popover"
			id="category-popover"
			data-testid="category-popover"
			role="dialog"
			aria-label="Categories"
		>
			<input
				class="category-search"
				type="search"
				data-testid="category-search"
				aria-label="Search categories"
				placeholder="Search categories"
				bind:this={search}
				bind:value={query}
			/>

			<div class="category-popover-actions">
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

			{#if filtered.length === 0}
				<p class="category-no-match" data-testid="category-no-match" role="status">
					No categories match “{query.trim()}”.
				</p>
			{:else}
				<div class="filter-options" role="group" aria-label="Category options">
					{#each filtered as category (category.key)}
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
			{/if}
		</div>
	{/if}
</div>
