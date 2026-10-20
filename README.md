# Bifrost

A learning-first Rust library for translating JSON messages between API revisions.

Stage 1's basic translator is implemented. It supports one reversible change:
`/session/terminal/name` in v1 becomes `/session/terminal/title` in v2. Stage 1
remains active pending the developer's learning review. Stages 2–5 (JSON Pointer
rules, tries, revision chains, streaming, and benchmarks) are not implemented.

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
  variable. No custom parser, rule engine, or extra dependency is needed here.

Implementation: `src/lib.rs` exposes the API and revisions, `src/translate.rs`
contains path navigation and renaming, and `src/error.rs` defines errors.
See [the learning log](docs/learning-log.md) for decisions and verification.

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

No benchmarks or performance claims are included. Review Stage 1 and its Rust
ownership/error flow before authorizing Stage 2.
