//! Bifrost translates JSON messages between explicitly supplied API revisions.
//!
//! Stage 1 supports the reversible `session.terminal.name` ↔ `title` rename.
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

mod error;
mod translate;

pub use error::BifrostError;
pub use translate::translate;

/// A supported API revision, supplied by the caller rather than read from JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Revision {
    /// Terminal labels use `name`.
    V1,
    /// Terminal labels use `title`.
    V2,
}

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
