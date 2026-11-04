/**
 * The one keyboard-activation rule shared by the graph's title buttons.
 *
 * A real `<button>` already activates on Enter and Space; handling the keys
 * explicitly keeps the app's selection in one place and makes it observable
 * without a pointer. Preventing the default stops the browser's own activation
 * from firing a second, duplicate selection. Nothing here knows a work item, a
 * graph, or a component; it only turns the two activation keys into one call.
 */
export function activateOnKey(
	event: KeyboardEvent & { currentTarget: EventTarget & HTMLButtonElement },
	activate: (trigger: HTMLButtonElement) => void
): void {
	if (event.key !== 'Enter' && event.key !== ' ') return;
	event.preventDefault();
	activate(event.currentTarget);
}
