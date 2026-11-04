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
	import { CONTEXT_NOTE, type GraphNode } from '$lib/graph';

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
	/** The tree's own phrase, so both presentations mark context the same way. */
	const context = $derived(node.context);

	/**
	 * Activate the node from the keyboard.
	 *
	 * The title is a real button, so a browser would activate it on Enter and
	 * Space on its own; handling the keys here keeps the selection in one place
	 * and makes it observable without a pointer. Preventing the default stops
	 * the browser's own activation from firing a second, duplicate selection.
	 */
	function onTitleKeydown(
		event: KeyboardEvent & { currentTarget: EventTarget & HTMLButtonElement }
	): void {
		if (event.key !== 'Enter' && event.key !== ' ') return;
		event.preventDefault();
		onSelect(node.id, event.currentTarget);
	}
</script>

<!-- The node element itself is a plain container: the title button is the one
     interactive and focusable thing inside it. -->
<div
	class="graph-node"
	class:selected
	class:context
	data-testid={`graph-node-${node.id}`}
	data-node-id={node.id}
	data-state={node.state}
	data-depth={node.depth}
	data-parent-id={node.parentId ?? ''}
	data-graph-y={Math.round(graphY)}
	data-context={context ? 'true' : undefined}
>
	<div class="graph-node-line">
		<span class="category">
			<Shape shape={node.shape} label={`${node.category.name} category`} size={12} />
			<span class="category-name">{node.category.name}</span>
		</span>
		{#if context}
			<!-- Visible text, so the marking survives greyscale and a screenshot;
			     hidden from assistive technology because the title's name says it. -->
			<span
				class="context-marker"
				data-testid={`graph-context-marker-${node.id}`}
				aria-hidden="true">Context — outside the selection</span
			>
		{/if}
		<span class="lifecycle-status">{statusLabel(node.status)}</span>
	</div>

	<button
		class="graph-node-title"
		class:selected
		type="button"
		data-testid={`graph-select-${node.id}`}
		aria-current={selected ? 'true' : undefined}
		aria-label={context ? `${node.title} — ${CONTEXT_NOTE}` : undefined}
		onclick={(event) => onSelect(node.id, event.currentTarget)}
		onkeydown={onTitleKeydown}
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
