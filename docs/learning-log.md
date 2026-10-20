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

## 2026-10-21 — Stage 4 implementation

- The developer authorized Stage 4 implementation, commit, and push. Stage 3's
  verified implementation remains the revision-chain reference. No Stage 5
  benchmark harness or optimization claims were introduced.
- Read RFC 8259: https://www.rfc-editor.org/rfc/rfc8259, including grammar,
  numbers, strings/Unicode, duplicate interoperability, and parser limits.
- Added `apply_rules_bytes` and `CompiledChain::translate_bytes` with explicit
  `StreamingLimits`, validated output, structural copy counters, and typed
  `StreamingError`. The interface borrows bytes and returns a bounded buffer;
  incremental reader/writer I/O is intentionally not part of this first editor.
- Implemented forward-only recursive descent with a byte cursor. UTF-8 is
  validated up front; strings validate controls, escapes, hex quads, and surrogate
  pairs without allocating decoded value strings. Object keys decode for trie
  matching and duplicate detection. Numeric leaves use `serde_json::Number` to
  retain the reference implementation's range behavior. No full `Value` is built.
- The scanner validates exact boundaries for unrelated subtrees instead of
  searching for delimiters blindly. Only key spans become edits. After full
  validation, copy original byte slices and escaped replacement keys in order.
  This preserves whitespace, member order, literal/numeric spelling, and value
  escape spelling. Structural counters establish the unchanged-subtree path.
- Chose strict duplicate decoded-key rejection everywhere, including unchanged
  subtrees and identities, instead of reproducing Value's already-collapsed
  duplicates. This prevents ambiguous edits. Same key spellings in separate
  objects are allowed. Unpaired surrogates and a leading BOM are rejected.
- Existing object-only rule semantics remain intact. Numeric pointer tokens can
  match object keys, but editing through arrays still errors. Unrelated arrays
  are fully validated and copied. The Value oracle remains unchanged.
- Retained ordered passes per compiled trie step. Combining all hops into one
  scan could change rename/path semantics. Multiple buffers/passes are a clear
  tradeoff until measured evidence justifies another design.
- Deferred schema/collision errors until syntax validation completes; choose the
  earliest error in trie postorder (sorted decoded-token siblings, descendants
  before parents). This preserves reference error behavior despite arbitrary
  input member order. Syntax, duplicate, and limit failures take precedence.
- Defaults per pass: 16 MiB input/output, depth 64, one million value/key tokens,
  8 MiB encoded string tokens. A hard depth ceiling of 128 prevents callers from
  disabling recursion protection. Output size is checked before reserving its
  buffer; accumulated replacement bytes are also bounded before allocation.
  Input/token limits bound key sets and edit metadata, not a measured memory total.
- Learning: `Scanner<'a>` holds borrowed slices and limits; recursion tracks
  container state. Per-object decoded-key sets cost memory but make duplicate
  semantics explicit. Recoverable output/edit reservation errors are surfaced;
  limits reduce allocation pressure without promising an allocator/OOM guarantee.
- Added deterministic differential tests for every revision pair, generated
  nested documents (fixed LCG seed 0xB1F2057), syntax mutations/truncations,
  malformed input, deep objects/arrays, empty values, Unicode/escaping, duplicate
  keys, resource boundaries, copy counters, exact bytes, and ordered errors.
- During development, parser tests passed; Clippy flagged two collapsible nested
  conditions, which were corrected.
- Final verification passed: `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test` (2 unit tests,
  49 integration tests, 3 doctests), `cargo doc --no-deps`, and
  `git diff --check`. The 17 streaming tests include deterministic mutations and
  truncations compared against `serde_json` acceptance. No benchmarks were run.
- Stage 4 remains active for learning review: explain why unchanged bytes still
  need syntax validation, and why copy counters are not performance evidence.
  Stop before Stage 5.

## 2026-10-21 — Stage 5 benchmarks and measured optimization

- The developer authorized Stage 5 implementation, commit, and push. Started from
  a clean Stage 4 commit, ead40e24c4a73f8deb0edf4197f7104bb8fbfd83.
- Added a custom stable-Rust benchmark harness using Instant/black_box rather
  than Criterion, preserving every raw batch-mean sample and environment/source
  hashes. Fixture generator uses LCG seed 0xB1F2057 and sizes near 1 KiB, 20 KiB,
  and 1 MiB. It checks structural equivalence against the Value translator before
  measuring any case and verifies fixture size bounds and shared plan identity.
- The 14-case matrix varies rule-path depth, rule count, 0/1/3 hops, sequential
  shared clients (1/8/64), and object-heavy versus long-string subtrees attached
  to changed and unchanged paths. Shared clients are not concurrent clients.
- Measured cached-plan Value parse/clone/edit/serialize and byte-editor end-to-end
  work. Separately timed cold plan compilation (32 independent prebuilt
  registries per sample) and warm cache lookup. Rule setup is excluded from cold
  compile; timer overhead is not subtracted. A pilot revealed that single cold
  operations were too close to timer quantization, so final runs use batches.
- Added dhat 0.3.3 as a dev-only dependency in a separate allocation executable.
  This avoids adding an unsafe counting allocator ourselves and avoids
  contaminating latency measurements with allocation tracing. Each allocation
  case runs once in a new process, with fixture/plan setup outside profiling.
  Collected cumulative blocks/bytes and peak simultaneous live heap separately;
  these are not process RSS or allocator metadata measurements.
- Collected three 31-sample baseline and three 31-sample optimized runs; each
  sample is an adaptive batch mean, not an individual request observation.
  Preserved JSON, heap-profile allocation-site summaries, hardware/toolchain
  details, and a computed six-run summary in docs/benchmark-results.
- Allocation profiling of the baseline 20 KiB object case attributed 2,090 of
  2,341 blocks to String key decoding and key cloning. Made exactly one
  production optimization: use Cow<'a, str> to borrow unescaped validated keys
  while continuing to decode escaped keys into owned Strings. The duplicate-key
  set and comparisons preserve decoded-key semantics; diagnostics own keys only
  when necessary. No unsafe code, rule-order changes, or skipped validation.
- Three-run median byte latency changed from 87.35 to 51.36 us (41.2% lower) on
  the 20 KiB object fixture. Blocks fell from 2,341 to 222 (90.5%), cumulative
  bytes from 90,429 to 81,077 (10.3%), peak live heap stayed 20,769 bytes.
  Object-heavy cases improved about 36–43%; plain-string cases showed no measured
  benefit. Three-hop and large-string cases still favored the Value pipeline.
- Raw source hashes distinguish baseline/optimized code measured before this
  commit. Added --source-ref to reproduce the old library with current harness
  and Cargo.lock in an isolated temporary directory. A five-sample filtered
  reproduction passed and matched the baseline streaming source hash.
- docs/benchmarks.md documents methodology, all matrix medians, run spread,
  p05/p95 of batch means, throughput, allocations/live heap, evidence, regression
  caveats, and exclusions. Apple M3/16 GiB/macOS 27/Rust 1.96.0 is one local
  environment; no claims about request p99, concurrency, escaped-key performance,
  CPU profiles, or production-wide speed. Heap profiles identify allocation
  bottlenecks. No additional optimization was attempted after the measured win.
- Final verification passed: `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test` (2 unit tests,
  49 integration tests, 3 doctests), and `cargo doc --no-deps`.
  Python syntax compilation, summary regeneration, raw-data shape checks,
  source/harness/fixture hash checks, and `git diff --check` also passed.
  Completed three full 31-sample runs per variant, 28 separately instrumented
  case/method processes per variant, two heap profiles, and one isolated
  five-sample baseline reproduction. Stage 5's technical exit evidence is
  recorded in docs/benchmarks.md; the implementation is ready for review.
- Learning review remains: explain Cow ownership,
  latency batching versus request tails, and traffic versus peak live memory.
