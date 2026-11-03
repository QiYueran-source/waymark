import { cleanup } from '@testing-library/svelte';
import { afterEach } from 'vitest';

// Svelte Testing Library mounts into the shared jsdom document; unmount between
// tests so ids, `aria-current`, and localStorage handlers do not leak.
afterEach(() => {
	cleanup();
	localStorage.clear();
});
