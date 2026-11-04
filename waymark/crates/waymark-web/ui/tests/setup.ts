import { cleanup } from '@testing-library/svelte';
import { afterEach } from 'vitest';

// jsdom implements neither of these. The graph view needs both before it can
// mount: the rendering library reads a media query for its reduced-motion
// behaviour and observes its container to measure the viewport.
if (typeof globalThis.matchMedia !== 'function') {
	globalThis.matchMedia = ((query: string) => ({
		matches: false,
		media: query,
		onchange: null,
		addEventListener: () => {},
		removeEventListener: () => {},
		addListener: () => {},
		removeListener: () => {},
		dispatchEvent: () => false
	})) as unknown as typeof globalThis.matchMedia;
}

if (typeof globalThis.ResizeObserver !== 'function') {
	globalThis.ResizeObserver = class {
		observe(): void {}
		unobserve(): void {}
		disconnect(): void {}
	} as unknown as typeof globalThis.ResizeObserver;
}

// Svelte Testing Library mounts into the shared jsdom document; unmount between
// tests so ids, `aria-current`, and localStorage handlers do not leak.
afterEach(() => {
	cleanup();
	localStorage.clear();
});
