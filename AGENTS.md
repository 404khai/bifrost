# AGENTS.md — Bifrost

> Bifrost is a learning-first Rust library for transforming JSON messages between API revisions.
>
> **Current milestone: Stage 4 — Forward-only streaming JSON editor.** Stages 1–3 are implemented; Stage 4 was authorized on 2026-10-21. Stage 5 is planned, not implemented. Never claim a stage is complete unless its acceptance criteria have been met and verified.

## 1. Mission

Build **Bifrost**, a small, correct, benchmarked library that allows independently versioned clients and servers to exchange JSON messages by translating payloads between supported API revisions.

The long-term architecture takes inspiration from version-compatibility systems that use JSON Pointer rules, prefix tries, lazy compilation of revision chains, and forward-only JSON editing. **Bifrost is an independent educational implementation, not a reimplementation of Rex's unpublished internals.**

Our priorities, in order:

1. **Understanding:** the human developer should be able to explain the design and the Rust code.
2. **Correctness:** transforms must be deterministic and must not silently lose or corrupt data.
3. **Clear architecture:** simple, well-defined modules and invariants.
4. **Measured efficiency:** optimize only after establishing benchmarks and identifying costs.
5. **Scope discipline:** progress sequentially through the five milestones.

Out of scope unless explicitly requested: networking, RPC transports, a running server, authentication, UI, filesystem persistence, schema registries, procedural macros, unsafe optimizations, code generation, and full compatibility with every possible API migration.

## 2. How the coding agent should collaborate

This repository is a **guided learning project**, not an autonomous speedrun.

- At the start of every task, read this file, inspect the repository, identify the active stage, and briefly state what will change.
- Work only on the current stage and the task the human requested. **Do not start the next stage automatically.**
- For a new concept, explain *what problem it solves*, *the simplest approach*, *the tradeoff*, and *why it belongs at this stage*.
- Prefer small, reviewable increments over sweeping refactors. Show or describe relevant function signatures and invariants before adding substantial complexity.
- If the human asks for a hint or an exercise, **do not supply the complete solution**. Give one useful hint and wait for their attempt.
- If the human explicitly asks you to implement a feature, implement that feature; do not withhold working code behind compulsory quizzes.
- Explain unfamiliar Rust features where relevant: ownership, borrowing, `Result`, `Option`, enums, matching, lifetimes, iterators, traits, and allocation behavior.
- Do not invent results. State exactly which tests, linters, and benchmarks were executed and whether they passed.
- Document alternatives considered and important discoveries in `docs/learning-log.md` when that file exists; create it during Stage 1.
- Ask for clarification only when an unresolved choice would materially alter correctness or public API; otherwise make a conservative decision and document it.

### Definition of a useful response

When performing work, report:

1. **Goal:** the specific problem being addressed.
2. **Approach:** the simplest correct design and its tradeoffs.
3. **Changes:** affected files and core logic.
4. **Verification:** commands run, results, and untested cases.
5. **Learning checkpoint:** one or two concepts the human should understand.
6. **Next step:** the next item *within the current stage*, or a recommendation to review before advancing.

## 3. Technical ground rules

- Language: **stable Rust**, edition 2024 if supported by the installed toolchain; otherwise use the current stable edition and document the choice.
- Project type: a **library crate** (`cargo new bifrost --lib` for a new repository).
- Stage 1 dependency: `serde_json`; introduce `serde` directly only when needed.
- Add other dependencies only when a real requirement justifies them; explain why. `criterion` or equivalent is allowed in Stage 5 for benchmarks.
- Design public APIs with explicit inputs and outputs; use `Result` for failures rather than panic paths.
- Avoid `unwrap()`/`expect()` in library code for untrusted data. Their use in tightly controlled tests is acceptable.
- Avoid `unsafe` unless separately proposed, justified, and explicitly approved.
- Keep the library independent of terminal, agent, web, or transport frameworks.
- Preserve unknown JSON fields unless the migration rule explicitly removes or changes them.
- Do not silently overwrite fields when a rename would collide with an existing key.
- Do not silently perform a lossy or non-invertible downgrade. If a reverse mapping is impossible, make that limitation explicit and return an error where appropriate.
- Version ordering, transformation direction, and composition order must be deterministic.
- Handle nested JSON, arrays where applicable, Unicode, escaped strings, `null`, numbers, booleans, and malformed input according to each stage's specified contract.
- Optimize only with evidence; do not claim parity with Rex or quote its benchmarks as Bifrost's results.

## 4. Milestones and progression gates

Each stage has a goal, required work, tests, and an exit gate. Mark it complete only after all requirements for that stage pass. Record completion in `docs/learning-log.md` with the date, relevant commands, and findings.

### Stage 1 — Basic translator [IMPLEMENTED]

**Goal:** Parse a JSON value, rename a nested field for v1 → v2, implement the reverse v2 → v1 mapping, and serialize the result.

**Start with one intentionally small, reversible schema evolution:**

- v1 uses `/session/terminal/name`.
- v2 uses `/session/terminal/title`.
- All other fields are preserved.
- The calling code supplies the source and target revision; do not require a `version` field inside the document.

Example v1:

```json
{
  "session": {
    "terminal": {
      "name": "astrid",
      "status": "running"
    }
  },
  "metadata": {"trace_id": "abc-123"}
}
```

Expected v2:

```json
{
  "session": {
    "terminal": {
      "title": "astrid",
      "status": "running"
    }
  },
  "metadata": {"trace_id": "abc-123"}
}
```

**Suggested API** (may be refined with documented reasoning):

```rust
pub enum Revision { V1, V2 }

pub fn translate(
    input: &serde_json::Value,
    from: Revision,
    to: Revision,
) -> Result<serde_json::Value, BifrostError>;
```

**Behavior contract:**

- v1 → v2 renames the nested key `name` to `title` if present.
- v2 → v1 performs the inverse rename.
- Same-revision translation returns an equivalent, independent value.
- Missing optional target paths are left unchanged (no key is invented).
- If both old and new keys exist in the same target object, do not overwrite either; return a clear conflict error when a rename is requested.
- If an intermediate path exists but has an incompatible JSON type, return a clear schema error rather than pretending the transformation succeeded.
- Input `&Value` must not be mutated.
- Serialize with `serde_json::to_string` in examples/tests; don't build a custom parser yet.

**Implementation tasks:**

- [x] Initialize a small library crate with `serde_json` and a README.
- [x] Define revisions and a minimal error type.
- [x] Implement v1 → v2 translation.
- [x] Implement v2 → v1 translation.
- [x] Add unit and integration tests for nested JSON objects.
- [x] Add documentation showing both versions and the behavior contract.

**Required tests:**

- Forward rename of a nested field.
- Reverse rename of a nested field.
- Forward-then-reverse round trip for valid v1 inputs.
- Same-version operation.
- Unrelated nested properties are preserved.
- Missing path; empty object; explicit `null` value.
- Field collision; wrong intermediate type.
- Original input is unchanged.

**Exit gate:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass; the developer can explain borrowing a `Value`, cloning versus moving it, object mutation, and error propagation. Stop here for review before Stage 2.

### Stage 2 — JSON Pointer and trie [IMPLEMENTED]

**Goal:** Replace hardcoded paths with structured rules indexed by JSON Pointer path tokens.

**Tasks:**

- [x] Read RFC 6901 and implement a focused JSON Pointer tokenizer.
- [x] Handle the empty pointer (document root), empty path tokens, `/` separators, `~0` for `~`, and `~1` for `/`.
- [x] Reject invalid escape sequences such as `~2` and dangling `~`.
- [x] Implement a trie with Rust structs/enums and `HashMap<String, Node>` or an equivalent representation.
- [x] Insert rules and look up applicable paths.
- [x] Support checking whether **any descendant** rule exists, not merely exact matches.
- [x] Refactor Stage 1 to express its rename through the rule/trie API while retaining existing behavior.

**Required tests:** nested paths, common prefixes, escaping, root/empty tokens, nonexistent paths, and array-index tokens as path components. If array transformations are not yet supported, explicitly distinguish *tokenizing* an array path from *applying* it.

**Exit gate:** no hardcoded path navigation remains in the transformation engine; all Stage 1 tests and new trie tests pass; the developer can explain why a prefix trie helps skip unrelated subtrees. Stop for review.

### Stage 3 — Lazy compilation of revision chains [IMPLEMENTED]

**Goal:** Support a sequence of API revisions without eagerly compiling every possible source-target pair.

**Tasks:**

- [x] Define adjacent revision rules such as v1 ↔ v2, v2 ↔ v3, and v3 ↔ v4.
- [x] Specify explicit forward and reverse operations; **do not assume every rule is automatically invertible**.
- [x] Resolve a requested `from → to` path using consecutive revisions in the correct order.
- [x] Lazily compile the requested chain into an efficient reusable representation.
- [x] Cache compiled chains keyed by `(source_revision, target_revision, registry_generation)` or use another safe invalidation scheme.
- [x] Preserve transformation order when multiple rules interact (e.g., renaming a path twice); merging rules into a trie must not silently change semantics.
- [x] Reject unknown revisions or missing transitions with useful errors.

**Required tests:** multi-hop upgrades/downgrades, identity translations, missing revisions, two successive renames, conflicting rules, cache reuse, and invalidation when rules change.

**Exit gate:** requested chains compile on demand; supported transformations behave identically to sequential application; there is no eager all-pairs precomputation. Demonstrate with instrumentation or tests, not assumptions. Stop for review.

### Stage 4 — Forward-only streaming JSON editor [ACTIVE]

**Goal:** Transform only affected JSON paths while copying unaffected values directly into output, without materializing the entire input as `serde_json::Value`.

**Tasks:**

- [x] Understand lexical JSON tokens, string escaping, nested arrays/objects, and parser state.
- [x] Establish a clearly defined interface taking JSON bytes/readers and an output writer/buffer.
- [x] Implement a forward-only parser/editor with recursive descent or an equivalent stack-based design.
- [x] Consult the compiled trie when entering object keys or array elements.
- [x] When no rule exists in a subtree, identify its exact byte range and copy it unchanged, avoiding full deserialization.
- [x] Apply rename/edit operations while maintaining valid JSON syntax, separators, and key ordering policies.
- [x] Preserve rule ordering and collision/error semantics from earlier stages.
- [x] Define safeguards for maximum nesting, input size, and allocation pressure.
- [x] Keep a `serde_json` reference implementation or test oracle for differential tests.

**Required tests:** deeply nested objects/arrays, escaped quotes and backslashes, Unicode, empty collections, large unchanged subtrees, duplicate keys (explicitly documented policy), malformed JSON, limits, and differential equivalence against the reference translator. Unchanged bytes may be copied only after the editor has correctly established their structural boundaries and validated JSON syntax.

**Exit gate:** new editor passes correctness tests, avoids building a full `Value` for routine translation, and demonstrates unchanged-subtree fast paths. No performance claims before benchmarks. Stop for review.

### Stage 5 — Benchmark and optimize

**Goal:** Measure translation latency, throughput, memory usage, and allocation behavior with repeatable experiments.

**Tasks:**

- [ ] Add a reproducible benchmark harness (`criterion` is an acceptable choice).
- [ ] Generate realistic, deterministic JSON fixtures near 1 KB, 20 KB, and 1 MB.
- [ ] Compare full `serde_json` parse/edit/serialize against the streaming editor.
- [ ] Vary nesting depth, rule density, number of revision hops, changed versus unchanged subtree sizes, and number of clients sharing a compiled transformation.
- [ ] Separate cold compilation time, cache-hit translation time, and end-to-end latency.
- [ ] Measure allocations using a well-defined tool or counting allocator; measure memory separately where possible.
- [ ] Profile bottlenecks, make one change at a time, rerun benchmarks, and record results.
- [ ] Report median, spread/percentiles where measurable, environment, sample counts, and limitations.

**Required evidence:** bench scripts, fixture generator/seed, hardware/toolchain details, raw or summarized results, and a performance report in `docs/benchmarks.md`.

**Exit gate:** correctness suite remains green; benchmarks are repeatable; optimizations have measured benefit on at least one explicitly described workload and their regressions/tradeoffs are documented.

## 5. Suggested repository structure

Do not create every directory in advance. Start with the smallest useful Stage 1 structure, then evolve it:

```text
bifrost/
├── AGENTS.md
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs
│   ├── error.rs            # Stage 1
│   ├── revision.rs         # Stage 1
│   ├── translate.rs        # Stage 1
│   ├── pointer.rs          # Stage 2
│   ├── trie.rs             # Stage 2
│   ├── rules.rs            # Stage 2+
│   ├── registry.rs         # Stage 3
│   ├── compiler.rs         # Stage 3
│   └── streaming.rs        # Stage 4
├── tests/
│   ├── basic_translation.rs
│   ├── pointers.rs         # Stage 2
│   ├── revision_chains.rs  # Stage 3
│   └── streaming.rs        # Stage 4
├── benches/                # Stage 5
└── docs/
    ├── learning-log.md
    └── benchmarks.md       # Stage 5
```

These file names are guidance, not an excuse to over-engineer or precreate unneeded modules. Prefer cohesive, tested files to empty scaffolding.

## 6. Testing, quality, and safety

For substantive changes, run when available:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo doc --no-deps
```

During development, `cargo fmt` may be used before the check. Fix warnings introduced by the change. If the environment prevents testing, explain the blocker and do not claim success.

Protect these invariants:

- Never lose unrelated JSON data by accident.
- Never silently overwrite colliding keys.
- Never treat two structurally different JSON messages as equivalent merely because their text looks similar.
- Treat incoming JSON as untrusted input; reject invalid syntax and enforce reasonable limits when implementing streaming parsing.
- Keep version direction explicit; an upgrade and a downgrade may have different or incompatible semantics.
- A cache hit must not use a compiled chain built for a different registry state.
- A fast path is valid only if there are provably no relevant rules under that subtree.
- When making performance claims, name exactly what was measured and how.

## 7. Documentation and research

Prefer first-party references for foundational concepts:

- JSON Pointer (RFC 6901): https://www.rfc-editor.org/rfc/rfc6901
- JSON Patch (RFC 6902), for comparison: https://www.rfc-editor.org/rfc/rfc6902
- JSON syntax (RFC 8259): https://www.rfc-editor.org/rfc/rfc8259
- Rust Book: https://doc.rust-lang.org/book/
- Rust standard library: https://doc.rust-lang.org/std/
- serde_json: https://docs.rs/serde_json/

**Important:** JSON Pointer locates a value. It does **not** itself specify an edit operation. Rules, invertibility, conflict semantics, and compatibility behavior belong to Bifrost's own design.

## 8. Initial instruction for the first coding session

When asked to start Bifrost:

1. Inspect the repository and toolchain first.
2. Confirm Stage 1 is active.
3. Create only the minimal library scaffold if absent.
4. Explain `serde_json::Value`, Rust borrowing, and the planned function contract briefly.
5. Propose one small implementation exercise (forward nested rename) for the human to attempt, **unless they explicitly request autonomous implementation**.
6. Add tests and the reverse translation within Stage 1 as requested.
7. Do not proceed to JSON Pointer, trie, lazy compilation, or streaming until Stage 1 is reviewed and the human authorizes Stage 2.

**Bifrost's guiding principle:** Make the simple version correct, understand its limitations, and earn each optimization through evidence.
