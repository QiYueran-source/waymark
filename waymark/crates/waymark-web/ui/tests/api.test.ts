import { describe, expect, it } from 'vitest';

import { ApiFailure, diagnosticText, fetchHealth, fetchItem, fetchTree } from '$lib/api';
import {
	combinedTree,
	detail,
	failure,
	health,
	jsonResponse,
	success
} from './fixtures';

interface Recorded {
	url: string;
	init: RequestInit | undefined;
}

function stub(
	respond: (url: string) => Response,
	recorded: Recorded[] = []
): typeof fetch {
	return (async (input: RequestInfo | URL, init?: RequestInit) => {
		const url = String(input);
		recorded.push({ url, init });
		return respond(url);
	}) as unknown as typeof fetch;
}

async function caught(promise: Promise<unknown>): Promise<ApiFailure> {
	try {
		await promise;
	} catch (error) {
		if (error instanceof ApiFailure) return error;
		throw error;
	}
	throw new Error('the read was expected to fail');
}

describe('GET-only envelope client', () => {
	it('reads health, tree, and item through GET with no credentials', async () => {
		const recorded: Recorded[] = [];
		const fetchImpl = stub((url) => {
			if (url === '/api/health') return jsonResponse(success(health()));
			if (url.startsWith('/api/tree')) return jsonResponse(success(combinedTree));
			return jsonResponse(success(detail));
		}, recorded);

		await fetchHealth(fetchImpl);
		await fetchTree('combined', fetchImpl);
		await fetchItem('wm-bug-login', fetchImpl);

		expect(recorded.map((call) => call.url)).toEqual([
			'/api/health',
			'/api/tree?view=combined',
			'/api/item/wm-bug-login'
		]);
		for (const call of recorded) {
			expect(call.init?.method).toBe('GET');
			expect(call.init?.credentials).toBe('omit');
			expect(call.init?.body).toBeUndefined();
		}
	});

	it('URL-encodes item ids', async () => {
		const recorded: Recorded[] = [];
		const fetchImpl = stub(() => jsonResponse(success(detail)), recorded);
		await fetchItem('wm/../odd id', fetchImpl);
		expect(recorded[0].url).toBe('/api/item/wm%2F..%2Fodd%20id');
	});

	it('throws the error envelope code, message, and diagnostics', async () => {
		const fetchImpl = stub(() =>
			jsonResponse(
				failure('INCOMPLETE_PROJECTION', 'the project projection is incomplete', [
					'work/tasks/wm-broken.md: damaged'
				]),
				500
			)
		);
		const error = await caught(fetchTree('combined', fetchImpl));
		expect(error.code).toBe('INCOMPLETE_PROJECTION');
		expect(error.message).toBe('the project projection is incomplete');
		expect(error.diagnostics).toEqual(['work/tasks/wm-broken.md: damaged']);
	});

	it('falls back to the details diagnostics when _meta is absent', async () => {
		const envelope = failure('INCOMPLETE_PROJECTION', 'incomplete', ['from details']);
		delete envelope._meta;
		const fetchImpl = stub(() => jsonResponse(envelope, 500));
		const error = await caught(fetchTree('combined', fetchImpl));
		expect(error.diagnostics).toEqual(['from details']);
	});

	it('treats a malformed tree payload as a read failure', async () => {
		const fetchImpl = stub(() => jsonResponse(success({ view: 'combined' })));
		const error = await caught(fetchTree('combined', fetchImpl));
		expect(error.code).toBe('INVALID_RESPONSE');
	});

	it('rejects a tree payload whose view does not match the request', async () => {
		const fetchImpl = stub(() => jsonResponse(success({ ...combinedTree, view: 'grouped' })));
		const error = await caught(fetchTree('combined', fetchImpl));
		expect(error.code).toBe('INVALID_RESPONSE');
	});

	it('reports non-JSON replies without inventing data', async () => {
		const fetchImpl = stub(
			() => new Response('<html>bad gateway</html>', { status: 502, headers: { 'content-type': 'text/html' } })
		);
		const error = await caught(fetchTree('combined', fetchImpl));
		expect(error.code).toBe('HTTP_502');
		expect(error.message).toContain('non-JSON');
	});

	it('reports an unreachable service', async () => {
		const fetchImpl = (async () => {
			throw new Error('connection refused');
		}) as unknown as typeof fetch;
		const error = await caught(fetchTree('combined', fetchImpl));
		expect(error.code).toBe('NETWORK_ERROR');
		expect(error.message).toContain('connection refused');
	});

	it('reports a success envelope without data', async () => {
		const fetchImpl = stub(() => jsonResponse({ v: 1, ok: true }));
		const error = await caught(fetchHealth(fetchImpl));
		expect(error.code).toBe('INVALID_RESPONSE');
	});
});

describe('diagnostics rendering', () => {
	it('renders strings verbatim and structured entries as JSON', () => {
		expect(diagnosticText('plain')).toBe('plain');
		expect(diagnosticText({ file: 'a.md', reason: 'damaged' })).toBe(
			'{"file":"a.md","reason":"damaged"}'
		);
		expect(diagnosticText(42)).toBe('42');
	});
});
