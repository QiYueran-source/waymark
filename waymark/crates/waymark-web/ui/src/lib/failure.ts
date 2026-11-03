/** A read failure as the UI presents it: code, message, and diagnostics. */
export interface Failure {
	code: string;
	message: string;
	diagnostics: unknown[];
}
