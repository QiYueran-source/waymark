<script lang="ts">
	/**
	 * One node of the graph.
	 *
	 * It carries the same two visual languages the tree uses — the category's
	 * stable shape and name, and the work state's colour plus its word — and the
	 * blocked reason in prose. Nothing here is clickable except the title, which
	 * selects the item; there is no drag handle, no field, and no menu, because
	 * the graph is read-only.
	 */
	import Shape from './Shape.svelte';
	import { stateLabel, statusLabel, type WorkState } from '$lib/types';
	import type { GraphNode } from '$lib/graph';

	let {
		node,
		selected = false,
		graphY,
		onSelect
	}: {
		node: GraphNode;
		selected?: boolean;
		/** The top edge of this node's row in the drawing. */
		graphY: number;
		/** The node button passes itself, so focus can return to it later. */
		onSelect: (id: string, trigger?: HTMLElement) => void;
	} = $props();

	const stateWord = $derived(stateLabel(node.state));
	const state: WorkState = $derived(node.state);
</script>

<!-- The node element itself is a plain container: the title button is the one
     interactive and focusable thing inside it. -->
<div
	class="graph-node"
	class:selected
	data-testid={`graph-node-${node.id}`}
	data-node-id={node.id}
	data-state={node.state}
	data-depth={node.depth}
	data-parent-id={node.parentId ?? ''}
	data-graph-y={Math.round(graphY)}
>
	<div class="graph-node-line">
		<span class="category">
			<Shape shape={node.shape} label={`${node.category.name} category`} size={12} />
			<span class="category-name">{node.category.name}</span>
		</span>
		<span class="lifecycle-status">{statusLabel(node.status)}</span>
	</div>

	<button
		class="graph-node-title"
		class:selected
		type="button"
		data-testid={`graph-select-${node.id}`}
		aria-current={selected ? 'true' : undefined}
		onclick={(event) => onSelect(node.id, event.currentTarget)}
	>
		{node.title}
	</button>

	<div class="graph-node-line">
		<span class="state state-{state}" data-testid={`graph-state-${node.id}`}>
			{stateWord}
		</span>
	</div>

	{#if node.state === 'blocked' && node.blockedReason}
		<p class="blocked-reason" data-testid={`graph-blocked-reason-${node.id}`}>
			<span class="blocked-marker" aria-hidden="true">!</span>
			{node.blockedReason}
		</p>
	{/if}
</div>
