<script lang="ts">
	import Shape from './Shape.svelte';
	import ReferenceList from './ReferenceList.svelte';
	import type { DependencyEdge, ReferenceNode } from '$lib/tree';
	import { shapeFor, stateLabel, statusLabel, type TreeNode } from '$lib/types';

	let {
		node,
		depth,
		expanded,
		hasChildren,
		selected,
		dependenciesVisible,
		edges,
		references,
		onToggle,
		onSelect
	}: {
		node: TreeNode;
		depth: number;
		expanded: boolean;
		hasChildren: boolean;
		selected: boolean;
		dependenciesVisible: boolean;
		edges: DependencyEdge[];
		references: ReferenceNode[];
		onToggle: (id: string) => void;
		/** `trigger` is the row button itself, so focus can return to it on close. */
		onSelect: (id: string, trigger?: HTMLElement) => void;
	} = $props();

	const shape = $derived(shapeFor(node.category));
</script>

<li
	class="node"
	class:selected
	data-testid={`node-${node.id}`}
	data-depth={depth}
	data-state={node.state}
	style:--depth={depth}
>
	<div class="node-row">
		{#each Array(depth) as _, guideIndex (guideIndex)}
			<span class="guide" class:innermost={guideIndex === depth - 1} aria-hidden="true"></span>
		{/each}

		{#if hasChildren}
			<button
				class="toggle"
				type="button"
				aria-expanded={expanded}
				aria-label={`${expanded ? 'Collapse' : 'Expand'} ${node.title}`}
				data-testid={`toggle-${node.id}`}
				onclick={() => onToggle(node.id)}
			>
				<svg
					class="chevron"
					class:expanded
					viewBox="0 0 12 12"
					width="12"
					height="12"
					aria-hidden="true"
					focusable="false"
				>
					<path d="M4.25 2.5 8.25 6l-4 3.5" />
				</svg>
			</button>
		{:else}
			<span class="toggle-spacer" aria-hidden="true"></span>
		{/if}

		<div class="node-body">
			<div class="node-line">
				<span class="category" title={`${node.category.name} (${shape})`}>
					<Shape {shape} label={`${node.category.name} category`} size={12} />
					<span class="category-name">{node.category.name}</span>
				</span>

				<button
					class="node-title"
					class:selected
					type="button"
					data-testid={`select-${node.id}`}
					aria-current={selected ? 'true' : undefined}
					onclick={(event) => onSelect(node.id, event.currentTarget)}
				>
					{node.title}
				</button>

				<span class="lifecycle-status" data-testid={`status-${node.id}`}>
					{statusLabel(node.status)}
				</span>
				<span
					class="state state-{node.state}"
					data-testid={`state-${node.id}`}
					title="Derived from this item's dependencies"
				>
					{stateLabel(node.state)}
				</span>
			</div>

			{#if node.state === 'blocked' && node.blocked_reason}
				<p class="blocked-reason" data-testid={`blocked-reason-${node.id}`}>
					<span class="blocked-marker" aria-hidden="true">!</span>
					{node.blocked_reason}
				</p>
			{/if}
		</div>
	</div>

	{#if dependenciesVisible && edges.length > 0}
		<ul class="edges" aria-label={`Dependencies of ${node.title}`}>
			{#each edges as edge (edge.to)}
				<li
					class="dependency-edge"
					data-testid="dependency-edge"
					data-from={edge.from}
					data-to={edge.to}
				>
					<span class="dashed-line" aria-hidden="true"></span>
					<span class="edge-label">depends on</span>
					<span class="edge-target">{edge.toTitle ?? edge.to}</span>
					{#if edge.toTitle}
						<span class="edge-id">{edge.to}</span>
					{/if}
					{#if edge.crossCategory}
						<span class="edge-cross">other category</span>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}

	{#if dependenciesVisible}
		<ReferenceList {references} />
	{/if}
</li>
