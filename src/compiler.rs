use std::sync::Arc;

use serde_json::Value;

use crate::rules::apply_rules_in_place;
use crate::{BifrostError, Rename, Revision, Trie};

/// An immutable snapshot of an ordered translation plan.
///
/// Created on demand by `Registry::compile`. Each step retains its own trie;
/// merging tries could replace rules or change sequential rename semantics.
/// Holding this snapshot across registry updates intentionally retains its old
/// behavior. A new registry lookup always uses the current configuration.
#[derive(Debug)]
pub struct CompiledChain {
    pub(crate) from: Revision,
    pub(crate) to: Revision,
    pub(crate) hops: usize,
    pub(crate) steps: Vec<Arc<Trie<Rename>>>,
}

impl CompiledChain {
    /// Source revision of the plan.
    pub fn source(&self) -> Revision {
        self.from
    }

    /// Target revision of the plan.
    pub fn target(&self) -> Revision {
        self.to
    }

    /// Number of adjacent transitions, including explicitly empty transitions.
    pub fn hop_count(&self) -> usize {
        self.hops
    }

    /// Number of ordered trie passes; identity plans have zero passes.
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }

    /// Clone once, then apply every trie step in chain order to that owned copy.
    /// Identity plans return an independent clone without schema validation.
    ///
    /// # Errors
    /// Returns the same schema or collision error as sequential `apply_rules`.
    /// The caller's input remains unchanged even if a later step fails.
    pub fn translate(&self, input: &Value) -> Result<Value, BifrostError> {
        let mut output = input.clone();
        for step in &self.steps {
            apply_rules_in_place(&mut output, step)?;
        }
        Ok(output)
    }
}
