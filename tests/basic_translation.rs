use bifrost::{BifrostError, Revision, translate};
use serde_json::{Value, json};

fn directions() -> [(Revision, Revision, &'static str, &'static str); 2] {
    [
        (Revision::V1, Revision::V2, "name", "title"),
        (Revision::V2, Revision::V1, "title", "name"),
    ]
}

#[test]
fn forward_and_reverse_preserve_unrelated_nested_data() {
    let v1 = json!({
        "session": {"terminal": {"name": "astrid", "status": "running"},
                    "other": {"windows": [1, null, true]}},
        "metadata": {"trace_id": "abc-123", "nested": [{"name": "unchanged"}]}
    });
    let v2 = json!({
        "session": {"terminal": {"title": "astrid", "status": "running"},
                    "other": {"windows": [1, null, true]}},
        "metadata": {"trace_id": "abc-123", "nested": [{"name": "unchanged"}]}
    });
    let original = v1.clone();
    assert_eq!(translate(&v1, Revision::V1, Revision::V2), Ok(v2.clone()));
    assert_eq!(translate(&v2, Revision::V2, Revision::V1), Ok(v1.clone()));
    assert_eq!(v1, original);
}

#[test]
fn all_field_value_types_round_trip_in_both_directions() {
    for value in [
        Value::Null,
        json!("astrid 🦀 \"quoted\" \\ newline\n"),
        json!(true),
        json!(42),
        json!(-1.25),
        json!([null, {"name": "nested"}]),
        json!({"title": "nested", "enabled": false}),
    ] {
        for (from, to, source, destination) in directions() {
            let input = json!({"session": {"terminal": {source: value.clone()}}});
            let original = input.clone();
            let mut output = translate(&input, from, to).unwrap();
            assert_eq!(
                output,
                json!({"session": {"terminal": {destination: value.clone()}}})
            );
            assert_eq!(translate(&output, to, from), Ok(input.clone()));
            output["session"]["terminal"][destination] = json!("changed");
            assert_eq!(input, original);
        }
    }
}

#[test]
fn missing_paths_and_source_keys_are_unchanged() {
    for input in [
        json!({}),
        json!({"metadata": [null, true]}),
        json!({"session": {}}),
        json!({"session": {"terminal": {}}}),
        json!({"session": {"terminal": {"status": "running"}}}),
    ] {
        for (from, to, _, _) in directions() {
            assert_eq!(translate(&input, from, to), Ok(input.clone()));
        }
    }
    for (from, to, _, destination) in directions() {
        let input = json!({"session": {"terminal": {destination: null}}});
        assert_eq!(translate(&input, from, to), Ok(input.clone()));
    }
}

#[test]
fn identity_returns_an_independent_value_without_validation() {
    for revision in [Revision::V1, Revision::V2] {
        for input in [
            json!(null),
            json!([true, 1]),
            json!({"session": false}),
            json!({"session": {"terminal": {"name": "a", "title": "b"}}}),
        ] {
            let mut output = translate(&input, revision, revision).unwrap();
            assert_eq!(output, input);
            if let Some(array) = output.as_array_mut() {
                array.push(json!("new"));
                assert_eq!(input, json!([true, 1]));
            }
        }
        let input = json!({"nested": {"items": [1]}});
        let mut output = translate(&input, revision, revision).unwrap();
        output["nested"]["items"][0] = json!(2);
        assert_eq!(input, json!({"nested": {"items": [1]}}));
    }
}

#[test]
fn collisions_error_without_mutating_input_even_for_equal_or_null_values() {
    for (name, title) in [
        (json!("a"), json!("b")),
        (json!("same"), json!("same")),
        (Value::Null, Value::Null),
    ] {
        let input = json!({"session": {"terminal": {"name": name, "title": title}}});
        let original = input.clone();
        for (from, to, source, destination) in directions() {
            assert_eq!(
                translate(&input, from, to),
                Err(BifrostError::Conflict {
                    path: "/session/terminal".into(),
                    source: source.into(),
                    destination: destination.into(),
                })
            );
            assert_eq!(input, original);
        }
    }
}

#[test]
fn incompatible_types_error_at_each_path_without_mutating_input() {
    for wrong in [
        Value::Null,
        json!(false),
        json!(12),
        json!("text"),
        json!([]),
    ] {
        for (input, path) in [
            (wrong.clone(), ""),
            (json!({"session": wrong.clone()}), "/session"),
            (
                json!({"session": {"terminal": wrong.clone()}}),
                "/session/terminal",
            ),
        ] {
            let original = input.clone();
            for (from, to, _, _) in directions() {
                assert_eq!(
                    translate(&input, from, to),
                    Err(BifrostError::ExpectedObject { path: path.into() })
                );
                assert_eq!(input, original);
            }
        }
    }
}

#[test]
fn parse_translate_serialize_pipeline_and_malformed_input() {
    let text = r#"{"session":{"terminal":{"name":"astrid"}},"metadata":{"trace_id":"abc-123"}}"#;
    let input: Value = serde_json::from_str(text).unwrap();
    let output = translate(&input, Revision::V1, Revision::V2).unwrap();
    let serialized = serde_json::to_string(&output).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&serialized).unwrap(),
        json!({"session": {"terminal": {"title": "astrid"}},
                      "metadata": {"trace_id": "abc-123"}})
    );
    assert!(serde_json::from_str::<Value>(r#"{"session": }"#).is_err());
}
