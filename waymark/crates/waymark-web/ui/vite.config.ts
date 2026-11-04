import { randomUUID } from 'node:crypto';
import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, type Plugin } from 'vite';

/**
 * Emit the build id that the provenance manifest records.
 *
 * The id belongs to the frontend build, not to the manifest generator: a release
 * builds the frontend once and every Web binary embeds that tree, so the same id
 * travelling with the tree is what proves the binary and the manifest describe
 * one build. The path is the adapter-static output directory configured in
 * `svelte.config.js` (`pages: 'build'`). The plugin runs after SvelteKit's own
 * closeBundle, which is when the adapter has written `build/`.
 */
function waymarkBuildId(): Plugin {
	return {
		name: 'waymark-build-id',
		apply: 'build',
		closeBundle() {
			const build = resolve('build');
			mkdirSync(build, { recursive: true });
			writeFileSync(resolve(build, 'build-id.txt'), `${randomUUID()}\n`);
		}
	};
}

export default defineConfig({
	plugins: [sveltekit(), waymarkBuildId()]
});
