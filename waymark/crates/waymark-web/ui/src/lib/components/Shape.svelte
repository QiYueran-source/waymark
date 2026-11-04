<script lang="ts">
	import type { ShapeName } from '$lib/types';

	let {
		shape,
		label,
		size = 14
	}: { shape: ShapeName; label: string; size?: number } = $props();
</script>

<svg
	class="shape"
	data-shape={shape}
	role="img"
	aria-label={label}
	viewBox="0 0 16 16"
	width={size}
	height={size}
	focusable="false"
>
	{#if shape === 'circle'}
		<circle cx="8" cy="8" r="6" />
	{:else if shape === 'square'}
		<rect x="2.5" y="2.5" width="11" height="11" rx="1.5" />
	{:else if shape === 'triangle'}
		<polygon points="8,2 14.5,13.5 1.5,13.5" />
	{:else if shape === 'diamond'}
		<polygon points="8,1.5 14.5,8 8,14.5 1.5,8" />
	{:else if shape === 'hexagon'}
		<polygon points="8,1.5 13.6,4.75 13.6,11.25 8,14.5 2.4,11.25 2.4,4.75" />
	{:else if shape === 'star'}
		<polygon
			points="8,1.5 9.9,5.9 14.7,6.3 11,9.5 12.2,14.2 8,11.7 3.8,14.2 5,9.5 1.3,6.3 6.1,5.9"
		/>
	{/if}
</svg>
