// Static SPA: one entry route, rendered in the browser. The backend serves
// `index.html` at `/` and has no history fallback, so there are no deep links.
export const prerender = true;
export const ssr = false;
