<script lang="ts">
	import NodeRow from './NodeRow.svelte';
	import Shape from './Shape.svelte';
	import {
		buildGroupedIndex,
		buildIndex,
		dependencyEdges,
		flatten,
		groupedReferences,
		referencesByAnchor,
		type NodeIndex,
		type ReferenceNode
	} from '$lib/tree';
	import { shapeFor, type CategoryGroup, type TreeNode, type ViewName } from '$lib/types';

	let {
		view,
		roots,
		groups,
		expanded,
		dependenciesVisible,
		selectedId,
		onToggle,
		onSelect
	}: {
		view: ViewName;
		roots: TreeNode[];
		groups: CategoryGroup[];
		expanded: string[];
		dependenciesVisible: boolean;
		selectedId: string | null;
		onToggle: (id: string) => void;
		onSelect: (id: string) => void;
	} = $props();

	const expandedSet = $derived(new Set(expanded));

	const combinedIndex: NodeIndex = $derived(view === 'combined' ? buildIndex(roots) : new Map());
	const groupedIndex: NodeIndex = $derived(
		view === 'grouped' ? buildGroupedIndex(groups) : new Map()
	);
	const references: Map<string, ReferenceNode[]> = $derived(
		view === 'grouped' ? referencesByAnchor(groupedReferences(groups)) : new Map()
	);

	const noReferences: ReferenceNode[] = [];
</script>

{#if view === 'combined'}
	<ul class="tree" aria-label="Work item tree">
		{#each flatten(roots, expandedSet) as row (row.node.id)}
			<NodeRow
				node={row.node}
				depth={row.depth}
				expanded={row.expanded}
				hasChildren={row.hasChildren}
				selected={selectedId === row.node.id}
				{dependenciesVisible}
				edges={dependenciesVisible ? dependencyEdges(row.node, combinedIndex) : []}
				references={noReferences}
				{onToggle}
				{onSelect}
			/>
		{/each}
	</ul>
{:else}
	{#each groups as group (group.category.key)}
		<section
			class="group"
			data-testid={`group-${group.category.key}`}
			aria-labelledby={`group-heading-${group.category.key}`}
		>
			<h2 class="group-heading" id={`group-heading-${group.category.key}`}>
				<Shape
					shape={shapeFor(group.category)}
					label={`${group.category.name} category`}
					size={15}
				/>
				<span>{group.category.name}</span>
			</h2>
			<ul class="tree" aria-label={`${group.category.name} work items`}>
				{#each flatten(group.roots, expandedSet) as row (row.node.id)}
					<NodeRow
						node={row.node}
						depth={row.depth}
						expanded={row.expanded}
						hasChildren={row.hasChildren}
						selected={selectedId === row.node.id}
						{dependenciesVisible}
						edges={dependenciesVisible ? dependencyEdges(row.node, groupedIndex) : []}
						references={references.get(row.node.id) ?? noReferences}
						{onToggle}
						{onSelect}
					/>
				{/each}
			</ul>
		</section>
	{/each}
{/if}
