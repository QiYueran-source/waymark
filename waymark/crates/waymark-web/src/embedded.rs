//! The frontend embedded by `build.rs`.
//!
//! `build.rs` writes `embedded_assets.rs` into `OUT_DIR` for every build: an
//! empty table and no manifest without the `embed` feature, and the gzipped
//! asset table plus the validated manifest with it. Keeping the generated file
//! unconditional means this module and the accessors in [`crate`] compile in
//! both variants; only the contents differ.

#![allow(dead_code)]

use std::sync::OnceLock;

use crate::manifest::AssetManifest;

include!(concat!(env!("OUT_DIR"), "/embedded_assets.rs"));

/// The provenance manifest this binary embeds, parsed once.
pub(crate) fn manifest() -> Option<&'static AssetManifest> {
    static PARSED: OnceLock<Option<AssetManifest>> = OnceLock::new();
    PARSED
        .get_or_init(|| {
            MANIFEST_JSON.map(|json| {
                serde_json::from_str(json)
                    .expect("the embedded manifest was validated by the build script")
            })
        })
        .as_ref()
}

/// The gzip bytes embedded for `path`, when this build embeds a frontend.
pub(crate) fn asset(path: &str) -> Option<&'static [u8]> {
    FILES
        .iter()
        .find(|file| file.path == path)
        .map(|file| file.gzip)
}
