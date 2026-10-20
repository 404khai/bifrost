# Bifrost

A learning-first Rust library for translating JSON messages between API revisions.

Stage 4's forward-only JSON byte editor is implemented alongside the `Value`
reference translator and lazy revision-chain cache. Stage 4 is active pending
learning review. Stage 5 (benchmarks and measured optimization) is not implemented.

The built-in revisions rename the field inside `/session/terminal`:

| Revision | Field |
| --- | --- |
| V1 | `name` |
| V2 | `title` |
| V3 | `label` |
| V4 | `display_name` |

Each adjacent upgrade and downgrade has an explicit rule. Multi-hop translations
apply those transitions in order, preserving unknown fields and the caller's input.

## Usage

This library uses stable Rust and edition 2024 (verified with Rust 1.96.0). From this checkout, run `cargo test` to try
the tests and executable documentation example.

```rust
use bifrost::{Revision, translate};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let v1: Value = serde_json::from_str(r#"{
        "session": {
            "terminal": {"name": "astrid", "status": "running"}
        },
        "metadata": {"trace_id": "abc-123"}
    }"#)?;

    let v2 = translate(&v1, Revision::V1, Revision::V2)?;
    assert_eq!(v2, json!({
        "session": {
            "terminal": {"title": "astrid", "status": "running"}
        },
        "metadata": {"trace_id": "abc-123"}
    }));

    let restored = translate(&v2, Revision::V2, Revision::V1)?;
    assert_eq!(restored, v1);
    println!("{}", serde_json::to_string(&v2)?);
    Ok(())
}
```

The caller supplies revisions; the JSON does not need a `version` field.

## Public API and behavior

```rust,ignore
pub enum Revision { V1, V2, V3, V4 }

pub fn translate(
    input: &serde_json::Value,
    from: Revision,
    to: Revision,
) -> Result<serde_json::Value, BifrostError>;
```

| Situation | Result |
| --- | --- |
| V1 → V2 | Rename `name` to `title` inside `session.terminal` |
| V2 → V1 | Rename `title` to `name` inside `session.terminal` |
| V1 → V4 | Apply `name` → `title` → `label` → `display_name` in order |
| V4 → V1 | Apply the explicitly configured reverse transitions in order |
| Same revision | Independent clone of any JSON value, without schema validation |
| Missing `session`, `terminal`, or source key | Unchanged independent value; no keys invented |
| Destination exists but source does not | Unchanged independent value |
| Both `name` and `title` exist | `BifrostError::Conflict`, even for equal or null values |
| Root, `session`, or `terminal` has a non-object type during a revision change | `BifrostError::ExpectedObject` |

All unrelated data is preserved, and the borrowed input is never mutated, even
on errors. A renamed field's value can be any JSON type, including null, objects,
and arrays. Arrays on the migration path are rejected; unrelated arrays are
preserved. A null field value is present data; a null intermediate value is a
schema error.

Errors implement `Display` and `std::error::Error`. Schema errors identify the
offending path (the empty string denotes the root); conflicts identify the
containing object and both key names.

Parsing and serialization belong to `serde_json`: malformed JSON fails before
translation. Structural values are preserved, rather than original whitespace or
object key order. The translator accepts an already parsed `Value`, so duplicate
keys have already been resolved by parsing and cannot be detected by the `Value`
API. The byte API below rejects duplicate decoded keys before returning output.

## Design and Rust learning notes

- `Value` is an enum representing JSON's possible types. Objects contain maps of
  strings to more `Value`s, allowing nested data.
- `&Value` is a shared borrow: the caller keeps ownership. `clone()` allocates an
  independent tree that the translator owns and may mutate. This deliberately
  favors a simple input-preservation guarantee over avoiding a full copy.
- `as_object_mut()` returns an `Option` containing a mutable map reference when
  the value is an object. `ok_or(...)?` converts absence into an error and returns
  that error to the caller. `get_mut()` distinguishes a missing key from an
  existing value with the wrong type.
- The collision check happens before removal. `remove()` returns ownership of
  the field value; `insert()` moves it into the destination without cloning the
  field again. The destination key needs an owned `String`.
- Revisions and errors are enums so callers can match explicit cases. `Copy`
  lets small revision values be passed by value without consuming the caller's
  variable. No custom JSON parser or extra dependency is needed here.

Implementation: `src/lib.rs` exposes the API; `src/revision.rs` defines revisions;
`src/translate.rs` provides a convenience wrapper over the built-in registry.
`src/pointer.rs` decodes pointer tokens,
`src/trie.rs` indexes values by token prefix, `src/rules.rs` applies edits without
schema-specific navigation, and `src/error.rs` defines translation errors.
`src/registry.rs` owns transitions and the cache; `src/compiler.rs` executes
immutable compiled plans. `src/streaming.rs` validates and edits JSON bytes.
See [the learning log](docs/learning-log.md) for decisions and verification.

## JSON Pointers and rule tries

`Pointer::parse` implements the string syntax in [RFC 6901](https://www.rfc-editor.org/rfc/rfc6901).
It accepts an already decoded Rust string; JSON string unescaping belongs to
`serde_json`. URI fragment syntax such as `#/path` is not accepted, and percent
escapes are not decoded.

- `""` denotes the root (zero tokens); `"/"` denotes the key `""` (one token).
- `/a~1b/~0` has tokens `a/b` and `~`. Escapes decode once: `~01` becomes `~1`.
- Missing leading slashes, `~2`, and dangling `~` return `PointerError`.
- Unicode and empty tokens are preserved without normalization.
- `/items/0` tokenizes to `items` and `0`. Tokenization alone does not interpret
  array indices. Applying a rule through an array currently returns
  `ExpectedObject`; numeric object keys work normally.

A pointer locates a value; Bifrost's `Rename` specifies the edit. In this API the
pointer locates the **containing object**, and source/destination are literal
field names:

```rust
use bifrost::{Pointer, Rename, Trie, apply_rules};
use serde_json::json;

let mut rules = Trie::new();
let object_path = Pointer::parse("/session/terminal")?;
assert_eq!(rules.insert(&object_path, Rename::new("name", "title")), None);
assert!(rules.get(&Pointer::parse("/session")?).is_none());
assert!(rules.has_descendant(&Pointer::parse("/session")?));

let input = json!({"session": {"terminal": {"name": "astrid"}}});
let output = apply_rules(&input, &rules)?;
assert_eq!(output, json!({"session": {"terminal": {"title": "astrid"}}}));
```

`Trie<T>` is generic: the same structure can index values of any type. Its
`BTreeMap<String, Trie<T>>` children share complete token prefixes and iterate in
deterministic order. `get` checks only an exact path; `has_descendant` checks
strict descendants; `has_rules_at_or_below` includes both. Nodes are created only
by insertion, so every child leads to a stored value. This invariant makes a
nonempty child map sufficient to detect descendant rules.

The engine follows trie children, looking up only the corresponding document
keys. A subtree with no trie branch needs no traversal or schema validation.
The entire JSON tree is still cloned; this is not streaming or a measured speedup.

There is one rename per object path. Inserting at an existing path returns and
replaces the old rule; it does not compose rules. Descendant edits run before
parent edits, and sibling branches run in lexicographic order of decoded tokens.
For example, a rule at `/a` edits that object before a root rule renames `a` to
`b`. Equal source/destination names are a no-op, while different colliding keys
remain errors. An empty trie independently clones any JSON value.

For arbitrary rule paths, `BifrostError` now owns its path and key `String`s
(previously static string references). Code constructing errors should use
`path: "/session".into()` and similar conversions. `InvalidPointer` wraps pointer
configuration errors, and `From<PointerError>` allows `?` to propagate them.
The original `translate` signature and V1/V2 behavior remain unchanged.

## Lazy revision chains and cache reuse

The standalone `translate` function builds a fresh built-in registry each call.
For repeated requests, keep a registry or reuse a compiled plan:

```rust
use bifrost::{Registry, Revision};
use serde_json::json;

fn main() -> Result<(), bifrost::BifrostError> {
    let mut registry = Registry::builtin()?;
    assert_eq!(registry.cache_stats().cached_chains, 0);
    let plan = registry.compile(Revision::V1, Revision::V4)?;
    assert_eq!(plan.hop_count(), 3);
    assert_eq!(plan.step_count(), 3);
    let input = json!({"session": {"terminal": {"name": "astrid"}}});
    assert_eq!(plan.translate(&input)?,
        json!({"session": {"terminal": {"display_name": "astrid"}}}));
    // Reuses the cached plan; no additional compilation.
    let _output = registry.translate(&input, Revision::V1, Revision::V4)?;
    assert_eq!(registry.cache_stats().compilations, 1);
    Ok(())
}
```

`compile` resolves consecutive numeric revisions only when a pair is requested.
It flattens the adjacent transitions' ordered steps into an immutable
`CompiledChain`, sharing the existing trie steps with `Arc`. It never merges
steps into one trie: `name` → `title` followed by `title` → `label` must remain
two passes. The compiled plan clones the input once, then edits that owned copy
in step order. Inside each trie pass, Stage 2's descendant-before-parent and
sorted-sibling policy still applies.

`Arc` means shared ownership of an immutable plan, so returning a cache hit does
not copy the plan or trie data. `compile` and registry mutation require
`&mut Registry`; the compiled handle can translate independently of a registry borrow.
A plan held across a rule update remains an intentional snapshot of the old
configuration. Fresh registry lookups use updated rules.

Cache keys are source/target pairs within one registry. Every successful
`set_transition` clears that registry's entire cache, including identity plans;
failed updates leave it untouched. This conservative invalidation replaces the
need for generation numbers and avoids stale cache entries. Separate registries
never share caches. `cache_stats` reports successful compilation count and current
cache size; tests also check shared handle identity with `Arc::ptr_eq`.

### Custom transitions and unsupported directions

`Registry::new([Revision::V1, Revision::V2, ...])` creates a registry with known
revisions but no transitions. The current revision vocabulary is fixed to v1–v4;
ordering is numeric regardless of registration order. `Revision::try_from(5_u32)`
returns `UnknownRevision`; a revision absent from a custom registry is also
unknown. Missing intermediate revisions are rejected instead of skipped.

`set_transition(from, to, Vec<Trie<Rename>>)` installs or replaces **one direction**
between consecutive revisions. Reverse operations must be supplied
separately. The vector's steps run in order, allowing multiple sequential edits
at the same path. An empty vector explicitly supports an unchanged transition;
a missing entry returns `MissingTransition`. Equal or nonadjacent endpoints
return `NonAdjacentTransition`. A configured direction does not prove its
reverse is possible, and Bifrost does not infer an inverse.

Identity plans contain no steps and clone any value without schema validation,
but the revision must still be known to the registry. Failed compilation is not
cached. Payload collisions or schema errors can occur during execution even when
a chain compiles; intermediate failures are preserved, rather than bypassed by
a direct first-to-last rename. A later failure never mutates the borrowed input.

There are no timing, throughput, or allocation benchmark results yet.
The plan representation favors explicit ordering over fusing passes; cache
invalidation favors a simple invariant over retaining unaffected entries.

## Forward-only JSON byte editing

Use a compiled plan to translate UTF-8 JSON bytes without building a full
`serde_json::Value` tree:

```rust
use bifrost::{Registry, Revision, StreamingLimits};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let plan = Registry::builtin()?.compile(Revision::V1, Revision::V4)?;
    let input = br#"{ "session": {"terminal": {"name": "astrid"}}, "keep": [1e+02] }"#;
    let output = plan.translate_bytes(input, &StreamingLimits::default())?;
    assert_eq!(output.bytes,
        br#"{ "session": {"terminal": {"display_name": "astrid"}}, "keep": [1e+02] }"#);
    assert_eq!(output.stats.renamed_keys, 3);
    Ok(())
}
```

For a custom trie, use `apply_rules_bytes(input, &rules, &limits)`. Both functions
return `StreamingOutput { bytes, stats }` or a `StreamingError`.

The recursive-descent scanner advances a byte cursor through objects, arrays,
strings, numbers, literals, and separators. It checks UTF-8 up front and validates
every subtree, including those with no rules. Only object keys and individual
numeric leaves are decoded with `serde_json`; string values and collection values
are not deserialized. The original `Value` engine remains available as a
reference implementation.

At each relevant object, the scanner consults decoded trie tokens and records
key replacement spans. Unrelated values take the validated-copy path: the scanner
establishes their exact boundaries and records copy counters, without constructing
their value trees. After the complete document passes validation, output assembly
copies slices in order and substitutes only renamed keys. Whitespace, member
order, numeric spellings, escaped strings, and all other unchanged bytes survive
exactly. New key names are escaped by `serde_json`.

This is a **buffered slice-to-buffer editor**, not incremental reader/writer I/O.
It holds the borrowed input, key sets for open objects, edit metadata, and the
output buffer. Multiple trie steps run as separate forward passes over byte
buffers, preserving Stage 3's exact composition order. Each step's output is the
next step's input; it does not build a full value tree. There are no partial
output writes exposed to the caller on errors.

### Byte API validation and error policy

The syntax follows [RFC 8259](https://www.rfc-editor.org/rfc/rfc8259), with an
explicit interoperability policy:

- Duplicate **decoded** keys are rejected in every object, including unchanged
  subtrees and identity translations. For example, `"a"` and `"\u0061"` collide.
  This intentionally differs from parsing into a `Value`, which has already
  collapsed duplicate keys.
- UTF-8, string escapes, Unicode surrogate pairs, separators, literals, and the
  end of the document are validated. Unpaired surrogates, a leading UTF-8 BOM,
  trailing data, and malformed content on unchanged paths are rejected.
- Numbers use `serde_json::Number` validation to match the reference engine's
  default range behavior, while preserving the original encoded bytes.
- Same-revision and explicitly empty plans still validate syntax and limits;
  they do not validate the migration schema and return an exact independent copy.
- Schema, missing-path, collision, and equal-name behavior matches the `Value`
  engine for valid unique-key JSON. Array values are validated and preserved;
  applying rules through arrays remains unsupported and returns `ExpectedObject`.
- Syntax, duplicate-key, and resource errors are reported when encountered,
  before a pending translation error. After successful syntax validation,
  translation errors retain the reference engine's sorted-child, descendant-first
  order, even if document members appear in another order.

`StreamingError` includes syntax offsets, duplicate-key information, named limit
failures, invalid limit configuration, recoverable buffer reservation failures,
and wrapped `BifrostError` values. Byte offsets refer to the pass that failed.

### Resource limits and copy evidence

`StreamingLimits::default()` applies these bounds independently to every pass:

| Limit | Default |
| --- | --- |
| Input bytes | 16 MiB |
| Output bytes | 16 MiB |
| Nested containers | 64 |
| Tokens (values plus object keys) | 1,000,000 |
| Encoded string token bytes, including quotes | 8 MiB |

Limits are configurable, except for a hard depth ceiling of 128 to protect the
recursive parser's stack. A scalar has depth zero. Limits also cover unchanged
subtrees and newly encoded keys. Total replacement-key bytes are bounded by the
output limit before accumulating edits; the final output size is checked before
reserving its buffer. Tokens and input size bound key-set and edit growth.
These are resource bounds, not a measured peak-memory or allocation guarantee.

`StreamingStats` counts passes, maximal copied value subtrees, their encoded
bytes (excluding surrounding whitespace), and renamed keys. Counts sum across
passes. A chain with zero trie steps performs one validation/copy pass. Tests
assert exact bytes and copy counters for large unchanged subtrees; these counters
demonstrate path selection, not speed or allocation measurements.

Learning checkpoint: `Scanner<'a>` borrows both input and limits rather than
owning or cloning them. Each recursive call holds parser state for one container.
The depth bound limits that stack; key sets enforce duplicate policy. Keeping a
pending semantic error lets the scan validate all syntax and then reproduce the
reference engine's order without rewinding input or materializing its tree.

## Verification

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo doc --no-deps
```

Tests cover forward/reverse renames, round trips, independent identity output,
unknown nested fields, absent paths, empty objects, all field value types,
Unicode and escaped strings, collisions, wrong intermediate types, unchanged
input on success and error, and parse/translate/serialize behavior.

Pointer and rule tests additionally cover root/empty tokens, escapes and invalid
syntax, common prefixes, missing paths, array-like tokens, generic edits,
deterministic edit order, and schema/conflict errors at dynamically encoded paths.

Revision-chain tests compare every built-in source/target pair against sequential
trie application, and cover identity plans, missing/unknown revisions, absent
reverse transitions, ordered edits, intermediate conflicts/errors, cache reuse,
invalidation, and independent registry caches.

Streaming tests cover all revision pairs, deterministic generated documents,
byte mutations and truncations against `serde_json`, exact copy spans, deep
objects/arrays, escaped keys and values, Unicode, empty collections, duplicate
keys, malformed syntax, limit boundaries, deterministic errors, and step order.

No benchmarks or performance claims are included. Review Stage 4 before
authorizing Stage 5.
