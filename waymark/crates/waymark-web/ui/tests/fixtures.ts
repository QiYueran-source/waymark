/**
 * Shared test fixtures: API envelopes, project trees, item details, and a
 * replaceable `fetch`.
 *
 * The shapes mirror `crates/waymark-web/src/api.rs`; nothing here is invented
 * by the UI. Tests assert presentation of what the server sends.
 */

import type {
	Category,
	CombinedTree,
	GroupedTree,
	Health,
	ItemDetail,
	TreeNode,
	ViewName
} from '$lib/types';

export const BUG: Category = { key: 'bug', name: 'Bugs', shape: 0 };
export const TASK: Category = { key: 'task', name: 'Tasks', shape: 1 };
/** Configured, but no active work item in it: the control must still offer it. */
export const DOCS: Category = { key: 'docs', name: 'Docs', shape: 2 };

export const ROOT_A = '/projects/alpha';
export const ROOT_B = '/projects/beta';

const taskChild: TreeNode = {
	id: 'wm-task-child',
	title: 'Write regression tests',
	category: TASK,
	status: 'in_progress',
	state: 'in_progress',
	ready: false,
	blocked: false,
	blocked_reason: null,
	parent: 'wm-bug-login',
	dependencies: [],
	children: []
};

export const bugRoot: TreeNode = {
	id: 'wm-bug-login',
	title: 'Login fails',
	category: BUG,
	status: 'in_progress',
	state: 'blocked',
	ready: false,
	blocked: true,
	blocked_reason: 'waiting on wm-task-ship (unfinished)',
	parent: null,
	dependencies: [],
	children: [taskChild]
};

export const taskRoot: TreeNode = {
	id: 'wm-task-ship',
	title: 'Ship the release',
	category: TASK,
	status: 'open',
	state: 'ready',
	ready: true,
	blocked: false,
	blocked_reason: null,
	parent: null,
	dependencies: ['wm-bug-login'],
	children: []
};

/** A tree node with neutral defaults, so a fixture states only what it means. */
export function treeNode(
	overrides: Pick<TreeNode, 'id' | 'title' | 'category'> & Partial<TreeNode>
): TreeNode {
	return {
		status: 'open',
		state: 'ready',
		ready: true,
		blocked: false,
		blocked_reason: null,
		parent: null,
		dependencies: [],
		children: [],
		...overrides
	};
}

/**
 * A three-level chain whose middle levels are in categories a reader can filter
 * out: a Docs root holds a Bug child, which holds Tasks and another Bug. The
 * chain is nested exactly as the server nests cross-category children (the
 * subtree is built before grouping), so filtering must keep the chain rather
 * than promote a Task to a root.
 */
export const docsSibling = treeNode({
	id: 'wm-docs-sibling',
	title: 'Rewrite the guide',
	category: DOCS,
	parent: 'wm-docs-umbrella'
});

export const bugSibling = treeNode({
	id: 'wm-bug-sibling',
	title: 'Crash on startup',
	category: BUG,
	parent: 'wm-bug-mid'
});

export const taskLeaf = treeNode({
	id: 'wm-task-leaf',
	title: 'Polish the leaf',
	category: TASK,
	parent: 'wm-bug-mid',
	status: 'in_progress',
	state: 'in_progress',
	ready: false
});

export const bugMid = treeNode({
	id: 'wm-bug-mid',
	title: 'Middle bug',
	category: BUG,
	parent: 'wm-docs-umbrella',
	status: 'in_progress',
	state: 'blocked',
	blocked: true,
	blocked_reason: 'waiting on wm-docs-umbrella (unfinished)',
	children: [taskLeaf, bugSibling]
});

export const docsRoot = treeNode({
	id: 'wm-docs-umbrella',
	title: 'Documentation umbrella',
	category: DOCS,
	children: [bugMid, docsSibling]
});

/**
 * The grouped payload for the deep chain: only Docs and Tasks are groups, so a
 * Task-only scope has to reach through unselected Docs and Bugs levels to keep
 * `wm-task-leaf` under its real parents.
 */
export const deepGroupedTree: GroupedTree = {
	view: 'grouped',
	groups: [
		{ category: DOCS, roots: [docsRoot] },
		{ category: TASK, roots: [taskRoot] }
	],
	unresolved: [],
	categories: [BUG, DOCS, TASK]
};

export const combinedTree: CombinedTree = {
	view: 'combined',
	roots: [bugRoot, taskRoot],
	unresolved: [],
	categories: [BUG, TASK]
};

/**
 * A deeper hierarchy with two roots: root → child → grandchild, plus a second
 * unrelated root. Used to check depth, placement, and that no synthetic root
 * is invented.
 */
const hierarchyGrandchild: TreeNode = {
	id: 'wm-h-grandchild',
	title: 'Verify the fix',
	category: TASK,
	status: 'open',
	state: 'ready',
	ready: true,
	blocked: false,
	blocked_reason: null,
	parent: 'wm-h-child',
	dependencies: [],
	children: []
};

const hierarchyChild: TreeNode = {
	id: 'wm-h-child',
	title: 'Reproduce the crash',
	category: BUG,
	status: 'in_progress',
	state: 'in_progress',
	ready: false,
	blocked: false,
	blocked_reason: null,
	parent: 'wm-h-root',
	dependencies: [],
	children: [hierarchyGrandchild]
};

const hierarchyRoot: TreeNode = {
	id: 'wm-h-root',
	title: 'Crashes on start',
	category: BUG,
	status: 'in_progress',
	state: 'blocked',
	ready: false,
	blocked: true,
	blocked_reason: 'waiting on wm-h-child (unfinished)',
	parent: null,
	dependencies: [],
	children: [hierarchyChild]
};

const hierarchySecondRoot: TreeNode = {
	id: 'wm-h-second-root',
	title: 'Document the release',
	category: TASK,
	status: 'open',
	state: 'ready',
	ready: true,
	blocked: false,
	blocked_reason: null,
	parent: null,
	dependencies: [],
	children: []
};

export const hierarchyTree: CombinedTree = {
	view: 'combined',
	roots: [hierarchyRoot, hierarchySecondRoot],
	unresolved: [],
	categories: [BUG, TASK]
};

export const groupedTree: GroupedTree = {
	view: 'grouped',
	groups: [
		{ category: BUG, roots: [bugRoot] },
		{ category: TASK, roots: [taskRoot] }
	],
	unresolved: [],
	categories: [BUG, TASK]
};

/**
 * The grouped payload's `categories` lists every configured category, including
 * one with no active work in this read; `groups` only lists categories that have
 * roots. The selection control is built from the former.
 */
export const groupedTreeWithEmptyCategory: GroupedTree = {
	...groupedTree,
	categories: [BUG, DOCS, TASK]
};

export const emptyTree: CombinedTree = {
	view: 'combined',
	roots: [],
	unresolved: [],
	categories: [BUG, TASK]
};

export const unresolvedTree: CombinedTree = {
	view: 'combined',
	roots: [bugRoot],
	unresolved: [
		{
			id: 'wm-orphan',
			title: 'Orphaned work',
			category: TASK,
			parent: 'wm-missing',
			reason: 'parent_missing'
		}
	],
	categories: [BUG, TASK]
};

export const detail: ItemDetail = {
	id: 'wm-bug-login',
	// Deliberately HTML-looking: titles are plain text and must never parse.
	title: '<b>Login</b> fails',
	body_html: '<p id="body-paragraph">Rendered <em>markdown</em> body.</p>',
	category: BUG,
	status: 'in_progress',
	state: 'blocked',
	ready: false,
	blocked: true,
	blocked_reason: 'waiting on wm-task-ship (unfinished)',
	owner: 'Ada Lovelace',
	creator: 'Grace Hopper',
	created_at: '2026-01-01T00:00:00Z',
	updated_at: null,
	closed_at: null,
	archived: false,
	custom_fields: {
		Severity: '<script>window.waymarkPwned = true;</script>',
		Notes: 'kept as text'
	},
	relationships: {
		parent: null,
		children: ['wm-task-child'],
		dependencies: [],
		dependents: ['wm-task-ship'],
		soft_links: []
	}
};

export function health(root: string = ROOT_A): Health {
	return {
		status: 'ok',
		service: 'waymark-web',
		version: '0.1.0',
		read_only: true,
		project: { root }
	};
}

/** A success envelope, matching `success_envelope`. */
export function success(data: unknown): Record<string, unknown> {
	return {
		v: 1,
		ok: true,
		data,
		_meta: {
			source: 'memory_projection',
			completeness: 'complete',
			freshness: 'unknown',
			diagnostics: [],
			warnings: []
		}
	};
}

/**
 * A diagnostic finding, matching `waymark_core::envelope::Finding`:
 * `{severity, code, location?, message, fixable}`.
 */
export interface Finding {
	severity: 'error' | 'warning' | 'info';
	code: string;
	location?: Record<string, unknown>;
	message: string;
	fixable: boolean;
}

/** One finding; `location` is omitted when the finding has no place. */
export function finding(options: {
	code: string;
	message: string;
	severity?: 'error' | 'warning' | 'info';
	location?: Record<string, unknown>;
	fixable?: boolean;
}): Finding {
	const value: Finding = {
		severity: options.severity ?? 'error',
		code: options.code,
		message: options.message,
		fixable: options.fixable ?? false
	};
	if (options.location !== undefined) value.location = options.location;
	return value;
}

/**
 * The `INCOMPLETE_PROJECTION` summary finding that leads a scoped projection's
 * diagnostics (`Projection::diagnostics`).
 */
export function incompleteProjectionSummary(
	options: { documents?: number; uncertain?: number } = {}
): Finding {
	const documents = options.documents ?? 0;
	const uncertain = options.uncertain ?? 0;
	return finding({
		code: 'INCOMPLETE_PROJECTION',
		message: `the projection is scoped and fresh: ${documents} document(s) could not be read and ${uncertain} item-shaped path(s) are uncertain`
	});
}

/**
 * A field-level validation finding for the document at `path`, as `parse.rs`
 * builds it: the location is the field, and the document is named in prose.
 */
export function validationFinding(path: string, field: string, reason: string): Finding {
	return finding({
		code: 'VALIDATION_ERROR',
		location: { kind: 'field', field },
		message: `work item document ${path}: ${field}: ${reason}`
	});
}

/** A document that could not be parsed; the location names the file. */
export function parseErrorFinding(path: string, reason: string): Finding {
	return finding({
		code: 'PARSE_ERROR',
		location: { kind: 'file', path },
		message: `cannot parse ${path}: ${reason}`
	});
}

/**
 * An item-shaped inventory path the projection could not classify
 * (`projection::inventory_finding`), which reuses `INCOMPLETE_PROJECTION`.
 */
export function inventoryUncertaintyFinding(path: string, reason: string): Finding {
	return finding({
		code: 'INCOMPLETE_PROJECTION',
		location: { kind: 'file', path },
		message: `the inventory is uncertain about ${path}: ${reason}`
	});
}

/** A failure envelope, matching `domain_error` / `adapter_error`. */
export function failure(
	code: string,
	message: string,
	diagnostics: unknown[] = []
): Record<string, unknown> {
	return {
		v: 1,
		ok: false,
		error: { v: 1, code, message, details: { diagnostics }, retryable: false },
		_meta: {
			source: 'memory_projection',
			completeness: 'scoped',
			freshness: 'unknown',
			diagnostics,
			warnings: []
		}
	};
}

export function jsonResponse(payload: unknown, status = 200): Response {
	return new Response(JSON.stringify(payload), {
		status,
		headers: { 'content-type': 'application/json' }
	});
}

export interface FetchOptions {
	/** The project root reported by `/api/health`. */
	root?: string;
	/** Tree payloads by view; a missing view fails the read. */
	trees?: Partial<Record<ViewName, CombinedTree | GroupedTree>>;
	/** When set, every tree read returns this failure envelope. */
	treeFailure?: Record<string, unknown>;
	/** Detail payloads by item id. */
	items?: Record<string, ItemDetail>;
	/** When set, every item read returns this failure envelope. */
	itemFailure?: Record<string, unknown>;
	/** When set, `/api/health` itself fails. */
	healthFailure?: Record<string, unknown>;
}

/** One request the UI made, method and URL, so tests can prove read-only. */
export interface RecordedRequest {
	method: string;
	url: string;
}

/** A browser `fetch` backed by fixed envelopes, recording requested URLs. */
export function makeFetch(options: FetchOptions): {
	fetchImpl: typeof fetch;
	urls: string[];
	requests: RecordedRequest[];
} {
	const urls: string[] = [];
	const requests: RecordedRequest[] = [];
	const impl = async (
		input: RequestInfo | URL,
		init?: RequestInit
	): Promise<Response> => {
		const raw = String(input);
		urls.push(raw);
		requests.push({ method: (init?.method ?? 'GET').toUpperCase(), url: raw });
		const url = new URL(raw, 'http://localhost:7331');
		if (url.pathname === '/api/health') {
			if (options.healthFailure) return jsonResponse(options.healthFailure, 500);
			return jsonResponse(success(health(options.root)));
		}
		if (url.pathname === '/api/tree') {
			if (options.treeFailure) return jsonResponse(options.treeFailure, 500);
			const view = (url.searchParams.get('view') ?? 'combined') as ViewName;
			const tree = options.trees?.[view];
			if (!tree) {
				return jsonResponse(failure('NOT_FOUND', `no fixture for the ${view} view`), 404);
			}
			return jsonResponse(success(tree));
		}
		if (url.pathname.startsWith('/api/item/')) {
			if (options.itemFailure) return jsonResponse(options.itemFailure, 500);
			const id = decodeURIComponent(url.pathname.slice('/api/item/'.length));
			const item = options.items?.[id];
			if (!item) {
				return jsonResponse(failure('ITEM_NOT_FOUND', `no fixture for ${id}`), 404);
			}
			return jsonResponse(success(item));
		}
		return new Response('not found', { status: 404 });
	};
	return { fetchImpl: impl as unknown as typeof fetch, urls, requests };
}
