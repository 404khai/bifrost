//! Object-key edits indexed by the pointer to their containing object.

use serde_json::Value;

use crate::pointer::escape_token;
use crate::{BifrostError, Trie};

/// Rename one key within the object located by a trie path.
/// Keys are literal strings, not JSON Pointer expressions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rename {
    pub source: String,
    pub destination: String,
}

impl Rename {
    /// Construct an explicit directional rename. Equal names are a no-op.
    pub fn new(source: impl Into<String>, destination: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            destination: destination.into(),
        }
    }
}

/// Clone the input and apply object-key renames indexed by containing-object paths.
///
/// Visits only rule-bearing trie branches; missing document keys are skipped.
/// Descendants run before their containing object's rename, with sibling branches
/// ordered lexicographically by decoded token. One edit is stored per path;
/// inserting again replaces it and returns the old edit. This is a tree-edit
/// policy, not sequential revision-chain composition.
///
/// # Errors
/// Returns a schema error on any existing non-object along a relevant branch
/// (including arrays), or a conflict if a rename would overwrite a different key.
/// The caller's input is preserved even if an earlier edit on the clone succeeded.
/// An empty trie accepts any JSON value without validation.
pub fn apply_rules(input: &Value, rules: &Trie<Rename>) -> Result<Value, BifrostError> {
    let mut output = input.clone();
    apply_node(&mut output, rules, "")?;
    Ok(output)
}

fn apply_node(value: &mut Value, node: &Trie<Rename>, path: &str) -> Result<(), BifrostError> {
    if !node.has_value_or_children() {
        return Ok(());
    }
    let object = value
        .as_object_mut()
        .ok_or_else(|| BifrostError::ExpectedObject {
            path: path.to_owned(),
        })?;
    for (token, child) in &node.children {
        if let Some(value) = object.get_mut(token) {
            let child_path = format!("{path}/{}", escape_token(token));
            apply_node(value, child, &child_path)?;
        }
    }
    if let Some(rule) = &node.value {
        if rule.source == rule.destination {
            return Ok(());
        }
        if object.contains_key(&rule.source) && object.contains_key(&rule.destination) {
            return Err(BifrostError::Conflict {
                path: path.to_owned(),
                source: rule.source.clone(),
                destination: rule.destination.clone(),
            });
        }
        if let Some(value) = object.remove(&rule.source) {
            object.insert(rule.destination.clone(), value);
        }
    }
    Ok(())
}
