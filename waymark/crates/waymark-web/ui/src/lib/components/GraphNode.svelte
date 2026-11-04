<script lang="ts">
	/**
	 * One node of the graph.
	 *
	 * It carries the same two visual languages the tree uses — the category's
	 * stable shape and name, and the work state's colour plus its word — and the
	 * blocked reason in prose. Two controls sit beside the title — the title itself,
	 * which selects the item, and, for a node with children, a toggle that hides
	 * or shows them. The toggle changes only what is drawn: no work item is
	 * edited, no relation is added or lost, and nothing is re-read. There is no
	 * drag handle, no field, and no menu, because the graph is read-only.
	 */
	import Shape from './Shape.svelte';
	import { activateOnKey } from '$lib/keys';
	import { stateLabel, statusLabel, type WorkState } from '$lib/types';
	import { graphNodeLabel, type GraphNode } from '$lib/graph';

	let {
		node,
		selected = false,
		collapsed = false,
		graphY,
		height,
		onSelect,
		onToggle,
		onMeasure
	}: {
		node: GraphNode;
		selected?: boolean;
		/** True when this node's children are hidden. */
		collapsed?: boolean;
		/** The top edge of this node's row in the drawing. */
		graphY: number;
		/** The height the layout used for this card. */
		height: number;
		/** The node button passes itself, so focus can return to it later. */
		onSelect: (id: string, trigger?: HTMLElement) => void;
		/** Collapse or expand this node's children. */
		onToggle: (id: string) => void;
		/** Report this card's rendered height back to the layout. */
		onMeasure: (id: string, height: number) => void;
	} = $props();

	const stateWord = $derived(stateLabel(node.state));
	const workState: WorkState = $derived(node.state);
	/** The tree's own phrase, so both presentations mark context the same way. */
	const context = $derived(node.context);

	/** The card element; observed so the layout learns its real height. */
	let card: HTMLDivElement | undefined = $state();

	/**
	 * Report the card's own border-box height.
	 *
	 * The card is content-sized, so only the browser knows how many lines a long
	 * title, blocked reason, or badge really took. It observes itself rather than
	 * being measured by the parent, so no layout query happens in the view and a
	 * resize or font change re-reports naturally. Before the first report the
	 * layout uses the deterministic estimate; a zero-sized box (an unrendered
	 * subtree) reports nothing.
	 */
	$effect(() => {
		const element = card;
		if (element === undefined || typeof ResizeObserver !== 'function') return;
		const observer = new ResizeObserver((entries) => {
			for (const entry of entries) {
				if (entry.target !== element) continue;
				const measured = entry.borderBoxSize?.[0]?.blockSize ?? entry.contentRect.height;
				if (measured > 0) onMeasure(node.id, measured);
			}
		});
		observer.observe(element);
		return () => observer.disconnect();
	});

	/**
	 * Activate the node from the keyboard.
	 *
	 * The title is a real button, so a browser would activate it on Enter and
	 * Space on its own; the shared helper keeps the selection in one place and
	 * makes it observable without a pointer, and preventing the default stops
	 * the browser's own activation from firing a second, duplicate selection.
	 */
	function onTitleKeydown(
		event: KeyboardEvent & { currentTarget: EventTarget & HTMLButtonElement }
	): void {
		activateOnKey(event, (trigger) => onSelect(node.id, trigger));
	}
</script>

<!-- The node element itself is a plain container: the title button is the one
     interactive and focusable thing inside it. -->
<div
	class="graph-node"
	class:selected
	class:context
	bind:this={card}
	data-testid={`graph-node-${node.id}`}
	data-node-id={node.id}
	data-state={node.state}
	data-depth={node.depth}
	data-parent-id={node.parentId ?? ''}
	data-graph-y={Math.round(graphY)}
	data-graph-height={Math.round(height)}
	data-collapsed={collapsed ? 'true' : 'false'}
	data-context={context ? 'true' : undefined}
	data-context-treatment={context ? 'outline' : undefined}
>
	<div class="graph-node-line">
		<span class="category">
			<Shape shape={node.shape} label={`${node.category.name} category`} size={12} />
			<span class="category-name">{node.category.name}</span>
		</span>
		<span class="lifecycle-status">{statusLabel(node.status)}</span>
	</div>

	<div class="graph-node-heading">
		<button
			class="graph-node-title"
			class:selected
			type="button"
			data-testid={`graph-select-${node.id}`}
			aria-current={selected ? 'true' : undefined}
			aria-label={context ? graphNodeLabel(node) : undefined}
			onclick={(event) => onSelect(node.id, event.currentTarget)}
			onkeydown={onTitleKeydown}
		>
			{node.title}
		</button>

		{#if node.hasChildren}
			<!-- The name states the action, so a screen reader announces what the
			     press will do rather than a bare "toggle". -->
			<button
				class="graph-node-toggle"
				type="button"
				data-testid={`graph-toggle-${node.id}`}
				aria-expanded={collapsed ? 'false' : 'true'}
				aria-label={`${collapsed ? 'Expand' : 'Collapse'} ${node.title}`}
				onclick={() => onToggle(node.id)}
			>
				<svg viewBox="0 0 10 10" aria-hidden="true" focusable="false">
					{#if collapsed}
						<path d="M3 1 L7 5 L3 9" />
					{:else}
						<path d="M1 3 L5 7 L9 3" />
					{/if}
				</svg>
			</button>
		{/if}
	</div>

	<div class="graph-node-line">
		<span class="state state-{workState}" data-testid={`graph-state-${node.id}`}>
			{stateWord}
		</span>
	</div>

	{#if context}
		<!-- Visible text, so the marking survives greyscale and a screenshot;
		     hidden from assistive technology because the title's name says it.
		     It is a line of the card, so it wraps and grows with the rest. -->
		<p
			class="context-marker graph-node-context"
			data-testid={`graph-context-marker-${node.id}`}
			data-context-badge="inside"
			aria-hidden="true">Context — outside the selection</p
		>
	{/if}

	{#if node.state === 'blocked' && node.blockedReason}
		<p class="blocked-reason" data-testid={`graph-blocked-reason-${node.id}`}>
			<span class="blocked-marker" aria-hidden="true">!</span>
			{node.blockedReason}
		</p>
	{/if}
</div>
