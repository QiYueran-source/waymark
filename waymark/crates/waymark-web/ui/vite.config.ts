import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, type Plugin } from 'vite';

/**
 * Emit the build id that the provenance manifest records.
 *
 * The id belongs to the frontend build, not to the manifest generator: a release
 * builds the frontend once and every Web binary embeds that tree, so the same id
 * travelling with the tree is what proves the binary and the manifest describe
 * one build. The id is emitted into the client bundle, so `adapter-static` copies
 * it into the configured `pages`/`assets` output (`build/`) with the rest of the
 * frontend; no Node filesystem API and no extra build step is involved.
 */
function waymarkBuildId(): Plugin {
	let clientBuild = false;
	return {
		name: 'waymark-build-id',
		apply: 'build',
		configResolved(config) {
			clientBuild = !config.build.ssr;
		},
		generateBundle() {
			if (!clientBuild) return;
			this.emitFile({
				type: 'asset',
				fileName: 'build-id.txt',
				source: `${crypto.randomUUID()}\n`
			});
		}
	};
}

export default defineConfig({
	plugins: [sveltekit(), waymarkBuildId()]
});
