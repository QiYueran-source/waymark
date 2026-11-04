import adapter from '@sveltejs/adapter-static';

/**
 * Static SPA output for the existing read-only Web service.
 *
 * The backend serves `GET /` from `<assets>/index.html`, `GET /assets/<path>`
 * from `<assets>/<path>`, and `GET /_app/<path>` from `<assets>/_app/<path>`,
 * and has no history fallback. So:
 * - there is exactly one prerendered entry route, `/`;
 * - `paths.base` stays `/`;
 * - `paths.assets` is deliberately unset: SvelteKit emits root-absolute
 *   `/_app/...` URLs, which is exactly the prefix the backend maps to
 *   `<build>/_app/...` (setting a root-relative `paths.assets` is rejected by
 *   Kit 2.70, and a non-root `base` would break the `/` entry route);
 * - `paths.relative` is `false` so `index.html` emits root-absolute `/_app/...`
 *   URLs rather than `./_app/...` URLs the backend cannot serve;
 * - no `fallback` page is emitted: deep links are 404s, as intended.
 */
/** @type {import('@sveltejs/kit').Config} */
const config = {
	kit: {
		adapter: adapter({
			pages: 'build',
			assets: 'build',
			strict: true
		}),
		paths: {
			relative: false
		}
	}
};

export default config;
