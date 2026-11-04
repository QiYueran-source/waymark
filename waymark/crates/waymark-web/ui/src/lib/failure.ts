/** A read failure as the UI presents it: code, message, and diagnostics. */
export interface Failure {
	code: string;
	message: string;
	diagnostics: unknown[];
}

/** One diagnostic that could be read as a finding. */
export interface LocatedProblem {
	/** The registered error code, kept so the cause keeps its identity. */
	code: string;
	/** The service's own sentence; the reason is never re-worded here. */
	message: string;
	/** The document path (with its position, when known) the finding names. */
	where: string | null;
	/** The field inside the document, when the finding names one. */
	field: string | null;
}

/** The diagnostics grouped by what blocked the projection. */
export interface DiagnosticReport {
	/** Findings about a document the projection could not use. */
	documents: LocatedProblem[];
	/** Findings about an item-shaped path the inventory cannot resolve. */
	uncertain: LocatedProblem[];
	/** Findings located somewhere else: a config key or a relationship. */
	other: LocatedProblem[];
	/** Diagnostics in no recognised shape: shown verbatim, never dropped. */
	leftovers: unknown[];
	/** Whether the read was blocked by the projection rather than by transport. */
	blocked: boolean;
}

/** The code the projection uses both for its own summary and for inventory doubts. */
const INCOMPLETE_PROJECTION = 'INCOMPLETE_PROJECTION';

function recordOf(value: unknown): Record<string, unknown> | null {
	return typeof value === 'object' && value !== null && !Array.isArray(value)
		? (value as Record<string, unknown>)
		: null;
}

function textOf(value: unknown): string | null {
	return typeof value === 'string' && value.length > 0 ? value : null;
}

function numberOf(value: unknown): number | null {
	return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function fileLabel(location: Record<string, unknown>): string | null {
	const path = textOf(location.path);
	if (path === null) return null;
	const line = numberOf(location.line);
	if (line === null) return path;
	const column = numberOf(location.column);
	return column === null ? `${path}:${line}` : `${path}:${line}:${column}`;
}

/** Where the finding points, as far as its location shape is understood. */
function placeIn(location: Record<string, unknown>): {
	where: string | null;
	field: string | null;
	aboutDocument: boolean;
} {
	switch (textOf(location.kind)) {
		// A document path, and the field inside it when the finding names one.
		case 'file':
			return { where: fileLabel(location), field: null, aboutDocument: true };
		// A field location has carried its document path since the projection
		// started recording it; the path and the field are shown separately.
		case 'field':
			return { where: fileLabel(location), field: textOf(location.field), aboutDocument: true };
		// Two documents claim one ID; the finding's message names the counterparts.
		case 'work_item':
			return { where: textOf(location.id), field: null, aboutDocument: true };
		case 'config_key':
			return { where: textOf(location.key), field: null, aboutDocument: false };
		case 'relationship': {
			const from = textOf(location.from);
			const relation = textOf(location.relation);
			const to = textOf(location.to);
			const where =
				from !== null && relation !== null && to !== null ? `${from} ${relation} ${to}` : from;
			return { where, field: null, aboutDocument: false };
		}
		default:
			return { where: null, field: null, aboutDocument: false };
	}
}

/**
 * Read one diagnostic as a finding, when it looks like one.
 *
 * Diagnostics are `Finding` values on the wire, but this block also has to
 * survive a plain string or a shape from another version of the service. A
 * diagnostic is only treated as a finding when its code *and* message are
 * non-empty strings; everything else is passed through untouched, so nothing is
 * either invented or dropped.
 */
function locatedProblem(value: unknown): (LocatedProblem & { aboutDocument: boolean }) | null {
	const finding = recordOf(value);
	if (finding === null) return null;
	const code = textOf(finding.code);
	const message = textOf(finding.message);
	if (code === null || message === null) return null;

	const location = recordOf(finding.location);
	const place =
		location === null ? { where: null, field: null, aboutDocument: false } : placeIn(location);
	return { code, message, ...place };
}

/**
 * The projection's summary message, when this diagnostic is it.
 *
 * That finding carries no location: it only restates the projection's
 * completeness and freshness and the two counts it is blocking on. It is set
 * aside rather than listed, because it names no document and blames no place:
 * the lists below are the causes, and repeating the counts as machine-worded
 * prose ("0 item-shaped path(s) are uncertain") would only add noise to them.
 */
function summaryOf(value: unknown): string | null {
	const finding = recordOf(value);
	if (finding === null || finding.code !== INCOMPLETE_PROJECTION) return null;
	if (typeof finding.location !== 'undefined') return null;
	return textOf(finding.message);
}

/**
 * Group a failure's diagnostics by what blocked the projection.
 *
 * The groups follow the codes the projection chose rather than any wording of
 * its messages, because the codes are the stable part of the wire:
 *
 * - `INCOMPLETE_PROJECTION` *with* a location is the inventory saying it cannot
 *   classify an item-shaped path (`projection::inventory_finding`), so it is the
 *   uncertainty group. The same code *without* a location is the count-bearing
 *   summary, which names nothing and is set aside.
 * - Every other located finding concerns a document: a validation or parse
 *   failure, an unreadable file, an ID/path mismatch, a duplicate claim. The
 *   inventory reuses `DUPLICATE_ID` and `PARSE_ERROR` for a case-variant path,
 *   and those stay here on purpose: a path that claims a live ID or is not the
 *   lowercase form of its own is something the reader must fix in a document.
 * - A finding located somewhere else (a config key, a relationship) is real but
 *   is not one of the two document causes, so it keeps its own group instead of
 *   being folded into one.
 *
 * An unknown code is still shown under `other`, never filed under a cause this
 * version does not actually understand, and a diagnostic that is not a finding
 * at all is left for the caller to render verbatim.
 */
export function readDiagnostics(failure: Failure): DiagnosticReport {
	const documents: LocatedProblem[] = [];
	const uncertain: LocatedProblem[] = [];
	const other: LocatedProblem[] = [];
	const leftovers: unknown[] = [];

	for (const diagnostic of failure.diagnostics) {
		if (summaryOf(diagnostic) !== null) continue;
		const problem = locatedProblem(diagnostic);
		if (problem === null) {
			leftovers.push(diagnostic);
		} else if (problem.code === INCOMPLETE_PROJECTION && problem.aboutDocument) {
			uncertain.push(problem);
		} else if (problem.aboutDocument) {
			documents.push(problem);
		} else {
			other.push(problem);
		}
	}

	return {
		documents,
		uncertain,
		other,
		leftovers,
		blocked:
			failure.code === INCOMPLETE_PROJECTION || documents.length > 0 || uncertain.length > 0
	};
}
