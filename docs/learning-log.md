# Learning log

## 2026-10-20 — Stage 1 scaffold

- Inspected `AGENTS.md` and the repository, which initially contained only that
  file. No Git repository was present; no Git repository was initialized.
- Installed toolchain: rustc 1.96.0, cargo 1.96.0. Selected edition 2024.
- Initialized a library crate with `serde_json` as its only direct dependency.
- Replaced Cargo's addition example with one nested JSON parse/serialize smoke
  test. Structural equality matters; JSON whitespace need not survive serialization.
- Design direction: borrow the caller's `Value` and clone it before editing.
  Cloning allocates for an independent tree but makes input preservation simple.
  Consuming the input could avoid that clone, but would change the intended API.
- Keep initial work in `src/lib.rs`; split modules when real implementation
  makes that useful. No pointer tokenizer, trie, or later-stage work is needed.
- Next: the learner attempts a forward rename on a known-valid fixture. This
  exercise does not satisfy the full translator behavior contract.
- Verification: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test`, and `cargo doc --no-deps` all passed. One unit test passed;
  no translation tests or benchmarks were run because translation is not yet
  implemented. Stage 1 remains incomplete.

## 2026-10-20 — Stage 1 translator implementation

- The developer explicitly requested full Stage 1 implementation, README updates,
  a conventional commit, and a push to `main`. A Git repository and `origin`
  remote now exist. No later-stage features were added.
- Kept `Revision` in `lib.rs`; separated translation and errors into cohesive
  modules. The public API matches the Stage 1 proposal.
- Clone the borrowed input once, then navigate with mutable object references.
  `get_mut` distinguishes absent keys from present non-object values;
  `ok_or(...)?` propagates schema errors. Rust ends each mutable borrow after
  its final use, allowing the owned output to be returned.
- The explicit revision match chooses forward and reverse key pairs. Check both
  keys before removal, then move the removed value into the new key. A null
  source value is present data and is renamed, not treated as absent.
- Conservative schema policy: during revision changes, the root and each
  present intermediate value must be objects. Error paths use the empty string
  for root. Same-revision calls clone without validation or collision checks.
  A destination-only object is unchanged because there is no source to rename.
- Rejected indexing for mutation because it can obscure absent paths and wrong
  types. Rejected a generic path/rule abstraction because that belongs to Stage
  2. No dependencies beyond `serde_json` were added.
- Initial `cargo test` passed: 2 unit tests, 7 integration tests, 1 doctest.
  Tests exercise both directions, every JSON field type, collision/schema
  failures, independent clones, input preservation, and serialization.
- Formal milestone completion still requires the developer's learning review:
  explain borrowing versus cloning, moving a removed value, mutable object
  access, and `?` propagation. Do not advance to Stage 2 without authorization.
- Final checks passed on rustc/cargo 1.96.0: `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test` (2 unit tests,
  7 integration tests, 1 doctest), and `cargo doc --no-deps`.
  No benchmarks were run; these belong to Stage 5.

## 2026-10-21 — Stage 2 implementation

- The developer authorized Stage 2. Stage 1's implementation/tests remain in
  place; advancing does not independently verify the human learning checkpoint.
- Read RFC 6901: https://www.rfc-editor.org/rfc/rfc6901 (syntax and evaluation).
  Implemented the plain string representation, not URI fragments or a general
  document evaluator. Root has zero tokens; `/` has one empty token. Split first,
  then decode escapes once to preserve `~01` as `~1`. Unicode is not normalized.
- Added `Pointer`, `PointerError`, generic `Trie<T>`, `Rename`, and `apply_rules`.
  The built-in translator now expresses the containing-object pointer and the
  explicit directional rename as data. No schema-specific navigation remains in
  the editing engine. No dependencies were added.
- Chose `BTreeMap` instead of `HashMap` for predictable branch/error ordering.
  This is a simplicity/determinism choice, not a benchmarked optimization.
  Children share full decoded tokens, not character prefixes.
- Trie insertion is the only way to create branches: every child reaches a
  value. With no deletion API, a nonempty child map proves a strict descendant
  value exists. Exact lookup and inclusive/strict prefix checks are distinct.
- One edit per path; insertion returns a replaced edit explicitly. Descendants
  run before parents; siblings run in sorted decoded-token order. This policy
  is documented/tested and must not be confused with Stage 3's future ordered
  revision-chain semantics. Equal-name renames are no-ops.
- Generic Rust learning: `Trie<T>` shares one implementation across value types;
  `Option<T>` distinguishes a node's stored value from prefix-only nodes.
  `entry(...).or_default()` creates missing nodes while borrowing the map mutably.
  Manual `Default` avoids an unnecessary `T: Default` constraint.
- Changed translation error path/key fields from static references to owned
  strings, enabling arbitrary escaped paths without lifetimes tied to the trie.
  Stage 1 test assertions only changed error field construction to `.into()`;
  their behavioral expectations are preserved. `From<PointerError>` enables `?`.
- Missing paths are unchanged, but present non-object values on rule paths error.
  Array-like tokens are accepted and indexed as strings; applying through arrays
  is explicitly unsupported and errors. Numeric object keys remain supported.
- Only trie-bearing branches are visited/validated by the engine; unrelated
  subtrees remain in the cloned output. Full-tree cloning is still required.
  No streaming, caching, revision chains, or performance claims were added.
- Initial tests passed: 2 unit tests, 20 integration tests, 1 doctest. New tests
  cover malformed escapes, root/empty tokens, Unicode/NUL, common prefixes,
  replacement, numeric tokens, custom paths, collision/schema errors, input
  preservation after partial clone edits, and deterministic application order.
- Final checks passed: `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test` (2 unit tests,
  20 integration tests, 1 doctest), `cargo doc --no-deps`, and
  `git diff --check`. No benchmarks were run.
- Stage 2 implementation is verified and ready for learning review. The checkpoint
  is explaining why `/session` may have no exact rule yet have a descendant rule,
  and why no trie branch means the engine can skip traversal of that subtree.
  Stop here before Stage 3.

## 2026-10-21 — Stage 3 implementation

- The developer explicitly authorized Stage 3 implementation, commit, and push.
  Stage 2's technical checks were green; human learning checkpoints remain review
  items, not something automated tests can certify. No Stage 4 work was added.
- Expanded the fixed revision enum to v1–v4, using numeric ordering and fallible
  `TryFrom<u32>` conversion. Built-in labels evolve name → title → label →
  display_name. Every forward/reverse adjacent transition is explicit data.
  This small enum avoids introducing arbitrary revision identifiers at this stage.
- Added `Registry` for known revisions, directional adjacent transitions, and a
  private per-registry source/target cache. Unknown endpoints/intermediates and
  missing transitions error. Registration order cannot change revision order;
  missing intermediate versions cannot be skipped. An empty step vector denotes
  an explicit unchanged transition, unlike an unsupported direction.
- A transition owns ordered trie steps. Compiling on demand resolves consecutive
  hops and flattens their steps into a reusable `CompiledChain` of shared `Arc`
  tries. No all-pairs precomputation occurs. Existing tokenization/trie indexing
  is reused; lazy compilation assembles the requested ordered execution plan.
- Rejected merging hops into one trie: repeated edits at the same path would
  replace each other, and path-renaming interactions could change semantics.
  Tests cover consecutive field renames and moving a parent before later edits
  inside its new location. Within a step, Stage 2's tree order remains unchanged.
- Plans clone the input once and use the internal in-place engine on that owned
  working copy for each step. Public `apply_rules` still clones its borrowed
  input. Errors on later passes discard only the working copy.
- Chose whole-cache invalidation on every successful transition update rather
  than registry generation counters. Configuration is owned/private and all
  writes require `&mut self`, so no cache entry can survive a rule change.
  Failed updates preserve both configuration and cache. Counters and shared
  pointer identity provide evidence for laziness, reuse, and invalidation.
- `Arc` provides shared ownership of immutable plans and tries without cloning
  their contents on cache hits. Explicitly held plans remain immutable snapshots
  after updates; new registry lookups compile against current rules. Registries
  have separate caches, preventing accidental cross-registry plan reuse.
- `translate` remains a convenience wrapper creating a fresh built-in registry.
  Callers wanting reuse keep a `Registry` or compiled plan. No global mutable
  state, locks, extra dependency, streaming, or benchmark claims were introduced.
- Initial tests passed: 2 unit tests, 32 integration tests, and 1 doctest. The
  12 new integration tests include all built-in pairs against sequential reference
  application, round trips, unknown/missing revisions, absent downgrade support,
  identity, ordering, intermediate conflicts/schema errors, snapshots, cache
  isolation, lazy reuse, failed compilation recovery, and cache invalidation.
- Final checks passed: `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test` (2 unit tests,
  32 integration tests, 2 doctests), `cargo doc --no-deps`, and
  `git diff --check`. No benchmarks were run.
- Stage 3 implementation is verified and ready for review. Review checkpoint:
  explain why merging revision steps can change behavior and why clearing all
  cached pairs after a mutation
  prevents stale plans. Stop here before Stage 4.
