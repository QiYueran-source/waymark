<script lang="ts">
	import type { ReferenceNode } from '$lib/tree';

	/**
	 * The cross-category reference anchor: a dashed leader, the referenced item,
	 * and the item in this group that depends on it. It sits under the
	 * referenced item's parent (or under the referenced item when it is a root),
	 * and exists only while dependency overlays are visible.
	 */
	let { references }: { references: ReferenceNode[] } = $props();
</script>

{#if references.length > 0}
	<ul class="references" aria-label="Cross-category references">
		{#each references as reference (reference.forId + '->' + reference.toId)}
			<li
				class="reference-node"
				data-testid="reference-node"
				data-reference-for={reference.forId}
				data-reference-to={reference.toId}
			>
				<span class="dashed-line" aria-hidden="true"></span>
				<span class="reference-label">
					{#if !reference.selfAnchored}
						<strong>{reference.toTitle}</strong>
						<span class="reference-note">
							referenced by <strong>{reference.forTitle}</strong>
						</span>
					{:else}
						<span class="reference-note">
							referenced by <strong>{reference.forTitle}</strong>
						</span>
					{/if}
				</span>
			</li>
		{/each}
	</ul>
{/if}
