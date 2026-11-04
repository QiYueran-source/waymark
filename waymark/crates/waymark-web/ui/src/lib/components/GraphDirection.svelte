<script lang="ts">
	/**
	 * The graph's reading direction: top to bottom, or left to right.
	 *
	 * It is a native radio group, so the selected direction is exposed to
	 * assistive technology as a checked choice with the words visible beside
	 * it, and the browser makes it keyboard-operable without a custom key
	 * handler. The control changes only how the hierarchy is laid out: it never
	 * filters, selects, collapses, or reads anything.
	 */
	import type { GraphDirection } from '$lib/graph';

	let {
		direction,
		onChange
	}: {
		direction: GraphDirection;
		onChange: (direction: GraphDirection) => void;
	} = $props();

	const OPTIONS: ReadonlyArray<{ value: GraphDirection; label: string }> = [
		{ value: 'TB', label: 'Top to bottom' },
		{ value: 'LR', label: 'Left to right' }
	];
</script>

<!-- A radio group, not a pair of toggle buttons: exactly one direction is
     always chosen, and the group and its checked option are announced. -->
<fieldset class="graph-direction" data-testid="graph-direction" data-direction={direction}>
	<legend class="sr-only">Graph direction</legend>
	{#each OPTIONS as option (option.value)}
		<label class="segment">
			<input
				type="radio"
				name="graph-direction"
				value={option.value}
				checked={direction === option.value}
				data-testid={`graph-direction-${option.value.toLowerCase()}`}
				onchange={() => onChange(option.value)}
			/>
			<span>{option.label}</span>
		</label>
	{/each}
</fieldset>
