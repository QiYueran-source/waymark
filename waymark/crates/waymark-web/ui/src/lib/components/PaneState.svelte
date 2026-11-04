<script lang="ts">
	/**
	 * The one wrapper for a blocking or empty state.
	 *
	 * Exactly five states may centre: no categories selected, selected
	 * categories with no active work, an empty project, a project read failure,
	 * and a detail-load failure inside the detail panel. They are the whole
	 * content of their pane, so they take the area the pane has and centre their
	 * message in it; no fixed height is assumed, because the graph canvas sizes
	 * the same pane.
	 *
	 * Non-blocking notices never use this component: they stay inline at the
	 * top. The `data-state-layout` marker is the test seam for that split, and
	 * `data-state-kind` names which of the five states is showing.
	 */
	import type { Snippet } from 'svelte';

	/** The five states that own their pane; no other kind may use this wrapper. */
	type PaneStateKind =
		| 'categories-cleared'
		| 'no-active-work'
		| 'empty-project'
		| 'read-failure'
		| 'detail-failure';

	let {
		testId,
		kind,
		children
	}: {
		testId: string;
		kind: PaneStateKind;
		children: Snippet;
	} = $props();

	/**
	 * The older `data-empty-kind` hook the filtered-empty contract uses.
	 *
	 * Only the two filtered-empty states carry it; the project-empty and the
	 * two failure states are not filter results and omit the attribute.
	 */
	const emptyKind = $derived(
		kind === 'categories-cleared' || kind === 'no-active-work' ? kind : undefined
	);
</script>

<div
	class="pane-state"
	data-testid={testId}
	data-state-layout="centered"
	data-state-kind={kind}
	data-empty-kind={emptyKind}
>
	{@render children()}
</div>
