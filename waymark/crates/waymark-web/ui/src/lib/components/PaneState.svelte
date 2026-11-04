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

	let {
		testId,
		kind,
		emptyKind,
		children
	}: {
		testId: string;
		kind: string;
		/** The older `data-empty-kind` hook the filtered-empty contract uses. */
		emptyKind?: string;
		children: Snippet;
	} = $props();
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
