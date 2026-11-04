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

export const combinedTree: CombinedTree = {
	view: 'combined',
	roots: [bugRoot, taskRoot],
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

/** A browser `fetch` backed by fixed envelopes, recording requested URLs. */
export function makeFetch(options: FetchOptions): {
	fetchImpl: typeof fetch;
	urls: string[];
} {
	const urls: string[] = [];
	const impl = async (input: RequestInfo | URL): Promise<Response> => {
		const raw = String(input);
		urls.push(raw);
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
	return { fetchImpl: impl as unknown as typeof fetch, urls };
}
