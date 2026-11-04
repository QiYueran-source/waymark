<script lang="ts">
	/**
	 * The graph's structured text alternative.
	 *
	 * It renders the outline the pure model builds from the *same* `WorkGraph`
	 * the drawing renders, so a category scope, a collapsed branch, or the
	 * dependency overlay cannot diverge between the two: there is one model and
	 * two readings of it. Nothing here draws an edge, a shape, or a colour — the
	 * parent-child and dependency relations are written out in words, and the
	 * hierarchy is stated in each entry rather than implied by indentation.
	 *
	 * The view is read-only. The only interactive element is the title button,
	 * which selects the item exactly as a graph card does, and passes itself so
	 * the existing focus-return path can put focus back on it.
	 */
	import { activateOnKey } from '$lib/keys';
	import type { OutlineEntry, OutlineItemEntry } from '$lib/graph';

	let {
		outline,
		selectedId = null,
		onSelect,
		onExpand,
		onShowAll
	}: {
		outline: OutlineEntry[];
		/** The item whose detail panel is open, if any. */
		selectedId?: string | null;
		/** Selecting an item; the trigger lets focus return after the panel closes. */
		onSelect: (id: string, trigger?: HTMLElement) => void;
		/**
		 * Expanding one folded branch from the list.
		 *
		 * It writes the same graph collapse set the drawing reads, so expanding
		 * here reopens the branch everywhere rather than only in this reading.
		 */
		onExpand: (id: string) => void;
		/** Restore the whole hierarchy: the graph-wide expand in words. */
		onShowAll: () => void;
	} = $props();

	const items = $derived(outline.filter((entry) => entry.kind === 'item'));
	const unresolved = $derived(outline.filter((entry) => entry.kind === 'unresolved'));

	/** The accessible name keeps the category and the state the card would show. */
	function label(entry: OutlineItemEntry): string {
		return `${entry.title} — ${entry.category}, ${entry.state}`;
	}

	/**
	 * Select from the keyboard.
	 *
	 * The title is a real button, so the browser would activate it on Enter and
	 * Space itself; the shared helper keeps selection in one place and makes it
	 * observable, and preventing the default stops a duplicate.
	 */
	function onTitleKeydown(
		event: KeyboardEvent & { currentTarget: EventTarget & HTMLButtonElement }
	): void {
		activateOnKey(event, (trigger) => {
			onSelect(trigger.dataset.nodeId ?? '', trigger);
		});
	}
</script>

<section class="graph-text" data-testid="graph-text" aria-labelledby="graph-text-heading">
	<h2 class="sr-only" id="graph-text-heading">Graph as a structured list</h2>
	<p class="graph-text-intro">
		The same work the drawing shows, written out. Every item states its category, its work
		state, its parent, and its children; each relation is named rather than shown.
	</p>

	<div class="graph-text-actions">
		<button
			type="button"
			class="graph-text-show-all"
			data-testid="graph-text-show-all"
			onclick={onShowAll}
		>
			Show all
		</button>
	</div>

	<ol class="graph-text-list">
		{#each items as entry (entry.id)}
			{#if entry.kind === 'item'}
				<li
					class="graph-text-item"
					class:selected={entry.id === selectedId}
					data-testid={`graph-text-item-${entry.id}`}
					data-node-id={entry.id}
					data-depth={entry.depth}
					data-parent-id={entry.parentId ?? ''}
				>
					<div class="graph-text-head">
						<button
							class="graph-text-title"
							class:selected={entry.id === selectedId}
							type="button"
							data-testid={`graph-text-select-${entry.id}`}
							data-node-id={entry.id}
							aria-label={label(entry)}
							aria-current={entry.id === selectedId ? 'true' : undefined}
							onclick={(event) => onSelect(entry.id, event.currentTarget)}
							onkeydown={onTitleKeydown}
						>
							{entry.title}
						</button>
						<span class="graph-text-id">{entry.id}</span>
					</div>

					<p class="graph-text-facts">
						Category:
						<span data-testid={`graph-text-category-${entry.id}`}>{entry.category}</span>. State:
						<span data-testid={`graph-text-state-${entry.id}`}>{entry.state}</span>. Status:
						{entry.status}.
					</p>

					<p class="graph-text-relation" data-testid={`graph-text-parent-${entry.id}`}>
						{entry.parent}
					</p>

					{#if entry.collapsed}
						<!-- A folded branch is a described row, not a silent gap: the
						     count says how much is hidden and the control reopens it
						     on the same graph collapse set the drawing reads. -->
						<div
							class="graph-text-collapsed"
							data-testid={`graph-text-collapsed-${entry.id}`}
							data-collapsed-children={entry.collapsedChildren}
						>
							<span>Collapsed — {entry.collapsedChildren} children</span>
							<button
								type="button"
								data-testid={`graph-text-expand-${entry.id}`}
								aria-label={`Expand ${entry.title}`}
								onclick={() => onExpand(entry.id)}
							>
								Expand
							</button>
						</div>
					{/if}

					{#if entry.children.length > 0}
						<ul class="graph-text-children" data-testid={`graph-text-children-${entry.id}`}>
							{#each entry.children as child (child)}
								<li>{child}</li>
							{/each}
						</ul>
					{/if}

					{#if entry.dependencies.length > 0}
						<ul class="graph-text-relations" data-testid={`graph-text-dependencies-${entry.id}`}>
							{#each entry.dependencies as dependency (dependency)}
								<li>{dependency}</li>
							{/each}
						</ul>
					{/if}

					{#if entry.dependents.length > 0}
						<ul class="graph-text-relations" data-testid={`graph-text-dependents-${entry.id}`}>
							{#each entry.dependents as dependent (dependent)}
								<li>{dependent}</li>
							{/each}
						</ul>
					{/if}

					{#if entry.contextNote}
						<p class="graph-text-context" data-testid={`graph-text-context-${entry.id}`}>
							{entry.contextNote}
						</p>
					{/if}

					{#if entry.blockedReason}
						<p class="graph-text-reason" data-testid={`graph-text-blocked-reason-${entry.id}`}>
							Blocked: {entry.blockedReason}
						</p>
					{/if}
				</li>
			{/if}
		{/each}
	</ol>

	{#if unresolved.length > 0}
		<h3 class="graph-text-heading">Unresolved relations</h3>
		<p class="graph-text-intro">
			These items exist but are not placed in the hierarchy, because their parent cannot be
			reached. They are named here rather than drawn as roots.
		</p>
		<ul class="graph-text-list" data-testid="graph-text-unresolved-list">
			{#each unresolved as entry (entry.id)}
				{#if entry.kind === 'unresolved'}
					<li
						class="graph-text-item graph-text-unresolved"
						data-testid={`graph-text-unresolved-${entry.id}`}
						data-node-id={entry.id}
					>
						<div class="graph-text-head">
							<span class="graph-text-title">{entry.title}</span>
							<span class="graph-text-id">{entry.id}</span>
						</div>
						<p class="graph-text-facts">
							Category:
							{entry.category}.
						</p>
						<p class="graph-text-reason">{entry.reason}</p>
					</li>
				{/if}
			{/each}
		</ul>
	{/if}
</section>
