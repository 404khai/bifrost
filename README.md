# Bifrost

A learning-first Rust library for translating JSON messages between API revisions.

Stage 2's JSON Pointer tokenizer, prefix trie, and object rename engine are
implemented. The built-in migration still renames `/session/terminal/name` in v1
to `/session/terminal/title` in v2. Stage 2 is active pending learning review;
stages 3–5 (revision-chain compilation, streaming, and benchmarks) are not
implemented.

## Usage

This library uses Rust edition 2024. From this checkout, run `cargo test` to try
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
pub enum Revision { V1, V2 }

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
keys have already been resolved by parsing and cannot be detected here.

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

Implementation: `src/lib.rs` exposes the API and revisions; `src/translate.rs`
configures the built-in migration. `src/pointer.rs` decodes pointer tokens,
`src/trie.rs` indexes values by token prefix, `src/rules.rs` applies edits without
schema-specific navigation, and `src/error.rs` defines translation errors.
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
The original `translate` signature and behavior remain unchanged. Rule tries
are constructed per call; no caching or revision-chain compilation is included.

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

No benchmarks or performance claims are included. Review Stage 2 before
authorizing Stage 3.
