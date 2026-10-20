//! Bifrost translates JSON messages between explicitly supplied API revisions.
//!
//! Stage 3 lazily compiles ordered revision chains from JSON Pointer rule tries.
//! Built-in terminal fields evolve through `name`, `title`, `label`, and `display_name`.
//!
//! ```
//! use bifrost::{Revision, translate};
//! use serde_json::json;
//!
//! let input = json!({"session": {"terminal": {"name": "astrid"}}});
//! let output = translate(&input, Revision::V1, Revision::V2)?;
//! assert_eq!(output, json!({"session": {"terminal": {"title": "astrid"}}}));
//! # Ok::<(), bifrost::BifrostError>(())
//! ```

mod compiler;
mod error;
mod pointer;
mod registry;
mod revision;
mod rules;
mod translate;
mod trie;

pub use compiler::CompiledChain;
pub use error::BifrostError;
pub use pointer::{Pointer, PointerError};
pub use registry::{CacheStats, Registry};
pub use revision::Revision;
pub use rules::{Rename, apply_rules};
pub use translate::translate;
pub use trie::Trie;

#[cfg(test)]
mod tests {
    use serde_json::Value;

    #[test]
    fn nested_json_survives_parsing_and_serialization() {
        let text = r#"{
            "session": {"terminal": {"name": "astrid", "status": "running"}},
            "metadata": {"trace_id": "abc-123"}
        }"#;
        let input: Value = serde_json::from_str(text).expect("fixture is valid JSON");

        assert_eq!(input["session"]["terminal"]["name"], "astrid");

        let serialized = serde_json::to_string(&input).expect("fixture can be serialized");
        let reparsed: Value = serde_json::from_str(&serialized).expect("output is valid JSON");
        assert_eq!(reparsed, input);
    }
}
