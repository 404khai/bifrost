use serde_json::Value;

use crate::{BifrostError, Revision};

/// Translate a JSON value without changing the caller's input.
///
/// V1 → V2 renames `session.terminal.name` to `title`; V2 → V1 reverses it.
/// Missing paths or source keys are unchanged. Field values may have any JSON
/// type. All unrelated data is preserved. Identity translations clone any value
/// without schema validation.
///
/// # Errors
///
/// Returns [`BifrostError::ExpectedObject`] if the root, `session`, or
/// `session.terminal` exists with a non-object type during a revision change.
/// Returns [`BifrostError::Conflict`] if both field names exist, even when their
/// values are equal or null.
pub fn translate(input: &Value, from: Revision, to: Revision) -> Result<Value, BifrostError> {
    let mut output = input.clone();
    let (source, destination) = match (from, to) {
        (Revision::V1, Revision::V2) => ("name", "title"),
        (Revision::V2, Revision::V1) => ("title", "name"),
        (Revision::V1, Revision::V1) | (Revision::V2, Revision::V2) => return Ok(output),
    };

    let root = output
        .as_object_mut()
        .ok_or(BifrostError::ExpectedObject { path: "" })?;
    let Some(session) = root.get_mut("session") else {
        return Ok(output);
    };
    let session = session
        .as_object_mut()
        .ok_or(BifrostError::ExpectedObject { path: "/session" })?;
    let Some(terminal) = session.get_mut("terminal") else {
        return Ok(output);
    };
    let terminal = terminal
        .as_object_mut()
        .ok_or(BifrostError::ExpectedObject {
            path: "/session/terminal",
        })?;

    // Check before removing anything: an existing destination must never be lost.
    if terminal.contains_key(source) && terminal.contains_key(destination) {
        return Err(BifrostError::Conflict {
            path: "/session/terminal",
            source,
            destination,
        });
    }
    if let Some(value) = terminal.remove(source) {
        terminal.insert(destination.to_owned(), value);
    }

    Ok(output)
}
