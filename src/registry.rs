use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde_json::Value;

use crate::{BifrostError, CompiledChain, Pointer, Rename, Revision, Trie};

/// Observable successful plan compilations and current cache size.
/// These counters describe behavior, not timing or benchmark results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheStats {
    pub compilations: usize,
    pub cached_chains: usize,
}

/// Explicit directional adjacent transitions and a cache local to this registry.
///
/// Transition steps are owned and immutable after registration. Every successful
/// transition update clears the entire cache. With all mutation behind `&mut
/// self`, cached pairs cannot refer to a previous registry state. Failed updates
/// leave configuration and cache untouched. No all-pairs compilation occurs.
///
/// ```
/// use std::sync::Arc;
/// use bifrost::{Registry, Revision};
/// use serde_json::json;
///
/// let mut registry = Registry::builtin()?;
/// assert_eq!(registry.cache_stats().compilations, 0);
/// let plan = registry.compile(Revision::V1, Revision::V4)?;
/// let again = registry.compile(Revision::V1, Revision::V4)?;
/// assert!(Arc::ptr_eq(&plan, &again));
/// assert_eq!(registry.cache_stats().compilations, 1);
/// let input = json!({"session": {"terminal": {"name": "astrid"}}});
/// assert_eq!(plan.translate(&input)?,
///     json!({"session": {"terminal": {"display_name": "astrid"}}}));
/// # Ok::<(), bifrost::BifrostError>(())
/// ```
#[derive(Debug)]
pub struct Registry {
    revisions: BTreeSet<Revision>,
    transitions: BTreeMap<(Revision, Revision), Vec<Arc<Trie<Rename>>>>,
    cache: BTreeMap<(Revision, Revision), Arc<CompiledChain>>,
    compilations: usize,
}

impl Registry {
    /// Create an empty transition registry supporting the supplied revisions.
    /// Revision order is numeric, independent of insertion order; duplicates are
    /// ignored. Missing intermediate revisions are errors, never skipped.
    pub fn new(revisions: impl IntoIterator<Item = Revision>) -> Self {
        Self {
            revisions: revisions.into_iter().collect(),
            transitions: BTreeMap::new(),
            cache: BTreeMap::new(),
            compilations: 0,
        }
    }

    /// Build v1 ↔ v2 ↔ v3 ↔ v4 terminal field renames, with no compiled chains.
    ///
    /// # Errors
    /// Propagates invalid built-in pointer or transition configuration.
    pub fn builtin() -> Result<Self, BifrostError> {
        let mut registry = Self::new([Revision::V1, Revision::V2, Revision::V3, Revision::V4]);
        let path = Pointer::parse("/session/terminal")?;
        // Reverse directions are explicit data, not automatically inferred.
        for (from, to, source, destination) in [
            (Revision::V1, Revision::V2, "name", "title"),
            (Revision::V2, Revision::V1, "title", "name"),
            (Revision::V2, Revision::V3, "title", "label"),
            (Revision::V3, Revision::V2, "label", "title"),
            (Revision::V3, Revision::V4, "label", "display_name"),
            (Revision::V4, Revision::V3, "display_name", "label"),
        ] {
            let mut step = Trie::new();
            step.insert(&path, Rename::new(source, destination));
            registry.set_transition(from, to, vec![step])?;
        }
        Ok(registry)
    }

    /// Install or replace one directional adjacent transition.
    ///
    /// `steps` execute in supplied order. Use separate steps for successive edits
    /// at the same path; inserting twice into one trie would replace an edit.
    /// An empty vector explicitly supports an unchanged transition; an absent
    /// entry is unsupported. No reverse transition is generated.
    ///
    /// # Errors
    /// Rejects revisions absent from this registry, equal revisions, and hops
    /// that skip a revision. Errors do not invalidate previously compiled plans.
    pub fn set_transition(
        &mut self,
        from: Revision,
        to: Revision,
        steps: Vec<Trie<Rename>>,
    ) -> Result<(), BifrostError> {
        self.require_revision(from)?;
        self.require_revision(to)?;
        if from.number().abs_diff(to.number()) != 1 {
            return Err(BifrostError::NonAdjacentTransition { from, to });
        }
        self.transitions
            .insert((from, to), steps.into_iter().map(Arc::new).collect());
        self.cache.clear();
        Ok(())
    }

    /// Resolve and flatten only the requested consecutive revision chain.
    /// Repeated requests share the same `Arc` until the registry changes.
    /// Identity plans have zero steps and still require a registered revision.
    ///
    /// # Errors
    /// Rejects unknown revisions or any missing directional transition on the
    /// chain. Failed compilation is not cached or counted as successful.
    pub fn compile(
        &mut self,
        from: Revision,
        to: Revision,
    ) -> Result<Arc<CompiledChain>, BifrostError> {
        self.require_revision(from)?;
        self.require_revision(to)?;
        if let Some(cached) = self.cache.get(&(from, to)) {
            return Ok(Arc::clone(cached));
        }

        let mut current = from;
        let mut steps = Vec::new();
        let mut hops = 0;
        while current != to {
            let number = if current < to {
                current.number() + 1
            } else {
                current.number() - 1
            };
            let next = Revision::try_from(number)?;
            self.require_revision(next)?;
            let transition =
                self.transitions
                    .get(&(current, next))
                    .ok_or(BifrostError::MissingTransition {
                        from: current,
                        to: next,
                    })?;
            steps.extend(transition.iter().map(Arc::clone));
            hops += 1;
            current = next;
        }
        let plan = Arc::new(CompiledChain {
            from,
            to,
            hops,
            steps,
        });
        self.cache.insert((from, to), Arc::clone(&plan));
        self.compilations = self.compilations.saturating_add(1);
        Ok(plan)
    }

    /// Compile or reuse a plan, then translate the borrowed input.
    ///
    /// # Errors
    /// Returns compilation errors or schema/collision errors from the plan.
    pub fn translate(
        &mut self,
        input: &Value,
        from: Revision,
        to: Revision,
    ) -> Result<Value, BifrostError> {
        self.compile(from, to)?.translate(input)
    }

    /// Cache size and lifetime count of successful compilations (saturating).
    pub fn cache_stats(&self) -> CacheStats {
        CacheStats {
            compilations: self.compilations,
            cached_chains: self.cache.len(),
        }
    }

    fn require_revision(&self, revision: Revision) -> Result<(), BifrostError> {
        if self.revisions.contains(&revision) {
            Ok(())
        } else {
            Err(BifrostError::UnknownRevision {
                revision: revision.number(),
            })
        }
    }
}
