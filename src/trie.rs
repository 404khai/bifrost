//! A token prefix tree. Only insertion creates nodes; no empty branches remain.

use std::collections::BTreeMap;

use crate::Pointer;

/// A deterministic prefix trie with at most one value at each pointer.
/// Children use decoded tokens, so `/a` and `/ab` do not share a token prefix.
#[derive(Debug, Clone)]
pub struct Trie<T> {
    pub(crate) value: Option<T>,
    pub(crate) children: BTreeMap<String, Trie<T>>,
}

impl<T> Default for Trie<T> {
    fn default() -> Self {
        Self {
            value: None,
            children: BTreeMap::new(),
        }
    }
}

impl<T> Trie<T> {
    /// Create an empty trie. `T` need not implement `Default`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a value, explicitly returning any value replaced at the exact path.
    /// This is map replacement, not composition of edit operations.
    pub fn insert(&mut self, pointer: &Pointer, value: T) -> Option<T> {
        let mut node = self;
        for token in pointer.tokens() {
            node = node.children.entry(token.clone()).or_default();
        }
        node.value.replace(value)
    }

    /// Look up only the exact pointer's value.
    pub fn get(&self, pointer: &Pointer) -> Option<&T> {
        self.node(pointer)?.value.as_ref()
    }

    /// Whether any strict descendant has a value; excludes the exact pointer.
    pub fn has_descendant(&self, pointer: &Pointer) -> bool {
        self.node(pointer)
            .is_some_and(|node| !node.children.is_empty())
    }

    /// Whether a value exists at this pointer or anywhere below it.
    pub fn has_rules_at_or_below(&self, pointer: &Pointer) -> bool {
        self.node(pointer).is_some_and(Self::has_value_or_children)
    }

    pub(crate) fn has_value_or_children(&self) -> bool {
        self.value.is_some() || !self.children.is_empty()
    }

    fn node(&self, pointer: &Pointer) -> Option<&Self> {
        let mut node = self;
        for token in pointer.tokens() {
            node = node.children.get(token)?;
        }
        Some(node)
    }
}
