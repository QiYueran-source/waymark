/**
 * The browser → API seam.
 *
 * Every response is the versioned envelope: `{v, ok, data, _meta}` on success
 * and `{v, ok: false, error, _meta?}` on failure. A failure is thrown as
 * [`ApiFailure`] carrying the registered code, message, and diagnostics so the
 * UI can show a read failure with its details instead of a partial tree.
 *
 * Only GET requests exist here. There is deliberately no request helper that
 * takes a method or a body: the read-only Web view has no write path at all.
 */

import type { Health, ItemDetail, TreeData, ViewName } from './types';

/** A structured read failure, from the API envelope or the network. */
export class ApiFailure extends Error {
	readonly code: string;
	readonly diagnostics: unknown[];

	constructor(code: string, message: string, diagnostics: unknown[] = []) {
		super(message);
		this.name = 'ApiFailure';
		this.code = code;
		this.diagnostics = diagnostics;
	}
}

interface ErrorEnvelope {
	ok?: boolean;
	error?: {
		code?: string;
		message?: string;
		details?: { diagnostics?: unknown };
	};
	_meta?: { diagnostics?: unknown };
}

interface SuccessEnvelope<T> {
	ok: boolean;
	data?: T;
}

/** Diagnostic entries are opaque; render strings as-is, everything else as JSON. */
export function diagnosticText(entry: unknown): string {
	if (typeof entry === 'string') return entry;
	try {
		return JSON.stringify(entry);
	} catch {
		return String(entry);
	}
}

function diagnosticsFrom(envelope: ErrorEnvelope): unknown[] {
	const fromMeta = envelope._meta?.diagnostics;
	if (Array.isArray(fromMeta)) return fromMeta;
	const fromDetails = envelope.error?.details?.diagnostics;
	if (Array.isArray(fromDetails)) return fromDetails;
	return [];
}

async function getJson<T>(fetchImpl: typeof fetch, url: string): Promise<T> {
	let response: Response;
	try {
		response = await fetchImpl(url, {
			method: 'GET',
			headers: { Accept: 'application/json' },
			// No credentials are ever needed; the service has no sessions.
			credentials: 'omit'
		});
	} catch (error) {
		throw new ApiFailure(
			'NETWORK_ERROR',
			`the Web service could not be reached: ${error instanceof Error ? error.message : String(error)}`
		);
	}

	let payload: unknown;
	try {
		payload = await response.json();
	} catch {
		throw new ApiFailure(
			response.ok ? 'INVALID_RESPONSE' : `HTTP_${response.status}`,
			`the Web service returned a non-JSON response (HTTP ${response.status})`
		);
	}

	const envelope = payload as ErrorEnvelope & SuccessEnvelope<T>;
	if (!response.ok || envelope.ok !== true) {
		const code = envelope.error?.code ?? `HTTP_${response.status}`;
		const message =
			envelope.error?.message ?? `the Web service refused the read (HTTP ${response.status})`;
		throw new ApiFailure(code, message, diagnosticsFrom(envelope));
	}
	if (envelope.data === undefined) {
		throw new ApiFailure('INVALID_RESPONSE', 'the Web service returned no data');
	}
	return envelope.data;
}

/** `GET /api/health`: liveness and the project root used for local isolation. */
export function fetchHealth(fetchImpl: typeof fetch = globalThis.fetch): Promise<Health> {
	return getJson<Health>(fetchImpl, '/api/health');
}

/** `GET /api/tree?view=…`. */
export function fetchTree(
	view: ViewName,
	fetchImpl: typeof fetch = globalThis.fetch
): Promise<TreeData> {
	const url = `/api/tree?view=${encodeURIComponent(view)}`;
	return getJson<TreeData>(fetchImpl, url).then((data) => {
		validateTree(view, data);
		return data;
	});
}

/** `GET /api/item/:id`. */
export function fetchItem(
	id: string,
	fetchImpl: typeof fetch = globalThis.fetch
): Promise<ItemDetail> {
	return getJson<ItemDetail>(fetchImpl, `/api/item/${encodeURIComponent(id)}`);
}

/**
 * Reject a payload that does not have the tree shape this UI presents.
 *
 * The server never returns a partial tree, so a malformed payload is treated as
 * a read failure — not rendered as an empty or truncated tree.
 */
function validateTree(view: ViewName, data: TreeData): void {
	if (!data || typeof data !== 'object' || data.view !== view) {
		throw new ApiFailure('INVALID_RESPONSE', `the ${view} tree payload has an unexpected view`);
	}
	if (!Array.isArray(data.unresolved) || !Array.isArray(data.categories)) {
		throw new ApiFailure(
			'INVALID_RESPONSE',
			'the tree payload is missing its unresolved items or categories'
		);
	}
	if (view === 'combined' && !Array.isArray((data as { roots?: unknown }).roots)) {
		throw new ApiFailure('INVALID_RESPONSE', 'the combined tree payload is missing roots');
	}
	if (view === 'grouped' && !Array.isArray((data as { groups?: unknown }).groups)) {
		throw new ApiFailure('INVALID_RESPONSE', 'the grouped tree payload is missing groups');
	}
}
