use serde_json::Value;

use crate::{BifrostError, Pointer, Rename, Revision, Trie, apply_rules};

/// Translate without mutating the input, using explicit forward/reverse rules.
/// Missing paths and source fields are unchanged; identity calls clone any value.
///
/// # Errors
/// Returns a schema error if the root, `session`, or `session.terminal` is not an
/// object during a revision change, or a conflict if both field names exist.
pub fn translate(input: &Value, from: Revision, to: Revision) -> Result<Value, BifrostError> {
    let rule = match (from, to) {
        (Revision::V1, Revision::V2) => Rename::new("name", "title"),
        (Revision::V2, Revision::V1) => Rename::new("title", "name"),
        (Revision::V1, Revision::V1) | (Revision::V2, Revision::V2) => return Ok(input.clone()),
    };
    // Schema knowledge belongs in rule configuration, not engine navigation.
    let path = Pointer::parse("/session/terminal")?;
    let mut rules = Trie::new();
    rules.insert(&path, rule);
    apply_rules(input, &rules)
}
