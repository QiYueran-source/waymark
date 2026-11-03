//! The adapter's only output shape: the shared versioned envelope.
//!
//! Every successful tool result carries `{v, ok: true, data, _meta}` and every
//! business failure carries `{v, ok: false, error}`. Both are built by
//! [`waymark_core::envelope`], so the MCP server cannot invent a second shape or
//! a second taxonomy.
//!
//! A business failure is a *tool result*, never a JSON-RPC error: the call
//! succeeded at the protocol level and the domain answered with a typed failure.
//! The result is marked `isError` so an MCP client surfaces it as a failed tool
//! call while a parser still receives the shared envelope on the content.
//! JSON-RPC errors are reserved for malformed requests, unknown tools, and
//! transport failures — the `rmcp` layer produces those, not this module.

use rmcp::model::CallToolResult;
use serde_json::Value;
use waymark_core::envelope::{failure_envelope, success_envelope, Meta};
use waymark_core::error::WaymarkError;

/// What every tool handler returns.
///
/// `Err` is a *protocol* failure: malformed parameters the tool macro rejected
/// before the handler ran, or a client that disappeared. A domain failure is
/// always `Ok` carrying a failure envelope.
pub type ToolResult = Result<CallToolResult, rmcp::ErrorData>;

/// A successful result carrying the shared envelope.
pub fn success(data: Value, meta: Meta) -> ToolResult {
    Ok(CallToolResult::structured(success_envelope(data, meta)))
}

/// A domain failure carrying the shared failure envelope.
pub fn business_failure(error: &WaymarkError) -> ToolResult {
    Ok(CallToolResult::structured_error(failure_envelope(error)))
}

/// Run a tool body, turning a domain [`WaymarkError`] into a failure envelope.
///
/// This is the one place a domain error crosses into the MCP representation, so
/// every tool reuses the registry classification rather than choosing a code.
pub fn respond(body: impl FnOnce() -> Result<ToolResult, WaymarkError>) -> ToolResult {
    match body() {
        Ok(result) => result,
        Err(error) => business_failure(&error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use waymark_core::envelope::ENVELOPE_VERSION;
    use waymark_core::error::Code;
    use waymark_core::WorkItemId;

    fn content(result: &CallToolResult) -> Value {
        let text = result
            .content
            .first()
            .and_then(|content| content.as_text())
            .expect("a structured result has a text content");
        serde_json::from_str(&text.text).expect("the content is the envelope JSON")
    }

    #[test]
    fn a_success_result_carries_the_shared_versioned_envelope() {
        let result = success(
            serde_json::json!({"answer": 42}),
            Meta::direct_file_scoped(),
        )
        .expect("a successful tool result");
        let value = content(&result);
        assert_eq!(value["v"], ENVELOPE_VERSION);
        assert_eq!(value["ok"], true);
        assert_eq!(value["data"]["answer"], 42);
        assert_eq!(value["_meta"]["source"], "direct_file");
        assert_eq!(result.structured_content.as_ref(), Some(&value));
        assert_eq!(result.is_error, Some(false));
    }

    #[test]
    fn a_domain_failure_is_a_tool_result_carrying_the_shared_failure_envelope() {
        let id = WorkItemId::parse("wm-000000000001").unwrap();
        let error = WaymarkError::item_not_found(&id);
        let result = business_failure(&error).expect("a business failure is not a protocol error");
        let value = content(&result);
        assert_eq!(value, failure_envelope(&error));
        assert_eq!(value["v"], ENVELOPE_VERSION);
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "ITEM_NOT_FOUND");
        assert_eq!(result.is_error, Some(true));
        assert_eq!(error.code(), Code::ItemNotFound);
    }
}
