//! `waymark version`: the release version and the capabilities of this build.
//!
//! The capability list is a compile-time fact, not a runtime probe: a basic
//! build lists `cli` and `mcp`, and only a Web build lists `web` and carries the
//! embedded frontend manifest. `--json` therefore answers both "which build is
//! this" and "which frontend tree does it embed" in the shared envelope.

use serde_json::{json, Map, Value};
use waymark_core::envelope::{success_envelope, Meta};
use waymark_core::error::WaymarkError;

use super::emit;

/// The workspace version this binary was built from.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The capabilities compiled into this build, in stable order.
fn capabilities() -> &'static [&'static str] {
    #[cfg(feature = "web")]
    {
        &["cli", "mcp", "web"]
    }
    #[cfg(not(feature = "web"))]
    {
        &["cli", "mcp"]
    }
}

/// Run `waymark version`.
pub fn run(json: bool) -> Result<(), WaymarkError> {
    if !json {
        println!("waymark {VERSION} ({})", capabilities().join(", "));
        return Ok(());
    }

    let mut data = Map::new();
    data.insert("version".to_owned(), json!(VERSION));
    data.insert("capabilities".to_owned(), json!(capabilities()));
    #[cfg(feature = "web")]
    {
        let manifest = waymark_web::embedded_manifest()
            .expect("an embed build always embeds its provenance manifest");
        data.insert(
            "assets".to_owned(),
            serde_json::to_value(manifest).map_err(|source| {
                WaymarkError::internal(format!("cannot serialize the embedded manifest: {source}"))
            })?,
        );
    }

    emit(&success_envelope(
        Value::Object(data),
        Meta::direct_file_scoped(),
    ))
}
