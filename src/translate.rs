use serde_json::Value;

use crate::{BifrostError, Registry, Revision};

/// Translate through the built-in v1–v4 revision sequence without mutating input.
///
/// Field names evolve `name` → `title` → `label` → `display_name`; downgrades
/// follow explicit reverse rules. Identity calls clone without schema validation.
/// This convenience function builds a fresh registry. Keep a `Registry` and use
/// its `translate` or `compile` methods to reuse plans across calls.
///
/// # Errors
/// Returns schema or collision errors from any intermediate step, or errors in
/// built-in rule configuration. Missing paths/source fields remain unchanged.
pub fn translate(input: &Value, from: Revision, to: Revision) -> Result<Value, BifrostError> {
    Registry::builtin()?.translate(input, from, to)
}
