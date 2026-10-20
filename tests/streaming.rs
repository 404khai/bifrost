use bifrost::{
    BifrostError, Pointer, Registry, Rename, Revision, StreamingError, StreamingLimits, Trie,
    apply_rules, apply_rules_bytes,
};
use serde_json::{Value, json};

fn rules(path: &str, from: &str, to: &str) -> Trie<Rename> {
    let mut trie = Trie::new();
    trie.insert(&Pointer::parse(path).unwrap(), Rename::new(from, to));
    trie
}

fn compare(input: &[u8], trie: &Trie<Rename>) {
    let value: Value = serde_json::from_slice(input).unwrap();
    let reference = apply_rules(&value, trie);
    let result = apply_rules_bytes(input, trie, &StreamingLimits::default());
    match reference {
        Ok(expected) => assert_eq!(
            serde_json::from_slice::<Value>(&result.unwrap().bytes).unwrap(),
            expected
        ),
        Err(expected) => assert_eq!(result.unwrap_err(), StreamingError::Translation(expected)),
    }
}

#[test]
fn rename_preserves_exact_whitespace_order_and_unrelated_bytes() {
    let input = br#" { "z": [ 1e+02, -0, true, null, {"q":"a\\b\"c"} ],
 "session" : { "terminal" : { "name" : "astrid", "status":"running" } }, "a":{} } "#;
    let trie = rules("/session/terminal", "name", "title");
    compare(input, &trie);
    let output = apply_rules_bytes(input, &trie, &StreamingLimits::default()).unwrap();
    let expected = String::from_utf8(input.to_vec())
        .unwrap()
        .replace("\"name\"", "\"title\"");
    assert_eq!(output.bytes, expected.as_bytes());
    assert_eq!(output.stats.renamed_keys, 1);
    assert_eq!(output.stats.passes, 1);
    assert_eq!(output.stats.copied_subtrees, 4);
}

#[test]
fn escaped_unicode_empty_and_pointer_keys_match_by_decoded_value() {
    let mut trie = rules("/a~1b/~0/", "n\u{0}é", "quote\" slash/ back\\\n\u{1}");
    trie.insert(
        &Pointer::parse("").unwrap(),
        Rename::new("root", "new_root"),
    );
    let input = br#"{"a\u002fb":{"~":{"":{"n\u0000\u00e9":"\uD83E\uDD80"}}},"root":null}"#;
    compare(input, &trie);
    let output = apply_rules_bytes(input, &trie, &StreamingLimits::default()).unwrap();
    assert_eq!(output.stats.renamed_keys, 2);
    assert!(
        String::from_utf8(output.bytes)
            .unwrap()
            .contains("\\uD83E\\uDD80")
    );
    compare("{\"🦀\":{\"é\":1}}".as_bytes(), &rules("/🦀", "é", "é"));
}

#[test]
fn every_revision_pair_matches_the_value_oracle_with_ordered_passes() {
    let revisions = [Revision::V1, Revision::V2, Revision::V3, Revision::V4];
    let fields = ["name", "title", "label", "display_name"];
    let mut registry = Registry::builtin().unwrap();
    for (index, from) in revisions.into_iter().enumerate() {
        for to in revisions {
            let plan = registry.compile(from, to).unwrap();
            for field in [
                Value::Null,
                json!(false),
                json!(-1.25),
                json!("🦀 \\ \" \n"),
                json!([]),
                json!({"deep": [1, true, null]}),
            ] {
                let input = json!({"session":{"terminal":{fields[index]:field,"status":"running"}},
                                  "metadata":[{"unchanged":true}]});
                let bytes = serde_json::to_vec(&input).unwrap();
                let output = plan
                    .translate_bytes(&bytes, &StreamingLimits::default())
                    .unwrap();
                assert_eq!(
                    serde_json::from_slice::<Value>(&output.bytes).unwrap(),
                    plan.translate(&input).unwrap()
                );
                assert_eq!(output.stats.passes, plan.step_count().max(1));
                if from == to {
                    assert_eq!(output.bytes, bytes);
                }
                assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), input);
            }
        }
    }
}

#[test]
fn ordered_steps_follow_parent_moves_and_preserve_intermediate_failures() {
    let mut registry = Registry::new([Revision::V1, Revision::V2, Revision::V3]);
    registry
        .set_transition(Revision::V1, Revision::V2, vec![rules("", "a", "b")])
        .unwrap();
    registry
        .set_transition(Revision::V2, Revision::V3, vec![rules("/b", "old", "new")])
        .unwrap();
    let plan = registry.compile(Revision::V1, Revision::V3).unwrap();
    for input in [
        json!({"a":{"old":1}}),
        json!({"a":{"old":1,"new":2}}),
        json!({"a":false}),
    ] {
        let bytes = serde_json::to_vec(&input).unwrap();
        match plan.translate(&input) {
            Ok(expected) => assert_eq!(
                serde_json::from_slice::<Value>(
                    &plan
                        .translate_bytes(&bytes, &StreamingLimits::default())
                        .unwrap()
                        .bytes
                )
                .unwrap(),
                expected
            ),
            Err(expected) => assert_eq!(
                plan.translate_bytes(&bytes, &StreamingLimits::default())
                    .unwrap_err(),
                StreamingError::Translation(expected)
            ),
        }
    }
    let plan = Registry::builtin()
        .unwrap()
        .compile(Revision::V1, Revision::V4)
        .unwrap();
    let input = br#"{"session":{"terminal":{"name":1,"title":2}}}"#;
    assert!(matches!(
        plan.translate_bytes(input, &StreamingLimits::default()),
        Err(StreamingError::Translation(BifrostError::Conflict { .. }))
    ));
}

#[test]
fn duplicate_decoded_keys_are_rejected_even_on_unchanged_paths_and_identity() {
    let trie = rules("/session/terminal", "name", "title");
    let identity = Registry::builtin()
        .unwrap()
        .compile(Revision::V1, Revision::V1)
        .unwrap();
    for input in [
        br#"{"a":1,"a":2}"#.as_slice(),
        br#"{"a":1,"\u0061":2}"#,
        br#"{"unrelated":[{"":true,"":false}]}"#,
        br#"{"session":{"terminal":{"name":1,"n\u0061me":2}}}"#,
    ] {
        assert!(matches!(
            apply_rules_bytes(input, &trie, &StreamingLimits::default()),
            Err(StreamingError::DuplicateKey { .. })
        ));
        assert!(matches!(
            identity.translate_bytes(input, &StreamingLimits::default()),
            Err(StreamingError::DuplicateKey { .. })
        ));
    }
    // Equal keys in separate objects do not collide.
    compare(br#"{"a":{"key":1},"b":{"key":2}}"#, &Trie::new());
}

#[test]
fn semantic_errors_match_sorted_postorder_instead_of_document_order() {
    let mut trie = rules("/a", "old", "new");
    trie.insert(&Pointer::parse("/z").unwrap(), Rename::new("old", "new"));
    compare(br#"{"z":false,"a":null}"#, &trie);
    trie.insert(&Pointer::parse("").unwrap(), Rename::new("a", "z"));
    compare(br#"{"z":{},"a":false}"#, &trie);
    let mut trie = rules("/a/b", "old", "new");
    trie.insert(&Pointer::parse("/a").unwrap(), Rename::new("old", "new"));
    compare(br#"{"a":{"old":1,"new":2,"b":false}}"#, &trie);
    // A schema error still validates the rest of the document before returning.
    assert!(matches!(
        apply_rules_bytes(
            br#"{"a":false,"z":[1,]}"#,
            &trie,
            &StreamingLimits::default()
        ),
        Err(StreamingError::InvalidJson { .. })
    ));
}

#[test]
fn collisions_in_both_key_orders_equal_names_and_missing_paths_match_reference() {
    for trie in [
        rules("/t", "old", "new"),
        rules("/t", "new", "old"),
        rules("/t", "old", "old"),
    ] {
        for input in [
            br#"{}"#.as_slice(),
            br#"{"t":{}}"#,
            br#"{"t":{"new":null}}"#,
            br#"{"t":{"old":null,"new":null}}"#,
            br#"{"t":{"new":1,"old":1}}"#,
            br#"{"t":null}"#,
            br#"{"t":[]}"#,
        ] {
            compare(input, &trie);
        }
    }
}

#[test]
fn deeply_nested_arrays_and_objects_copy_without_building_a_value_tree() {
    let mut bytes = Vec::new();
    for _ in 0..45 {
        bytes.extend_from_slice(b"[{\"a\":");
    }
    bytes.extend_from_slice(br#""escaped \" } ] \\ \u0061""#);
    for _ in 0..45 {
        bytes.extend_from_slice(b"}]");
    }
    let limits = StreamingLimits {
        max_depth: 100,
        ..StreamingLimits::default()
    };
    let output = apply_rules_bytes(&bytes, &Trie::new(), &limits).unwrap();
    assert_eq!(output.bytes, bytes);
    assert_eq!(output.stats.copied_subtrees, 1);
    assert_eq!(output.stats.copied_bytes, bytes.len());
    assert_eq!(output.stats.renamed_keys, 0);
}

#[test]
fn large_unchanged_subtree_is_copied_exactly_after_validation() {
    let payload = format!(
        "[ {}, {{\"raw\":1e+02}} ]",
        "null,".repeat(50_000).trim_end_matches(',')
    );
    let input =
        format!("{{\"session\":{{\"terminal\":{{\"name\":\"a\"}}}},\"payload\":{payload}}}");
    let output = apply_rules_bytes(
        input.as_bytes(),
        &rules("/session/terminal", "name", "title"),
        &StreamingLimits::default(),
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(output.bytes).unwrap(),
        input.replace("\"name\"", "\"title\"")
    );
    assert!(output.stats.copied_bytes >= payload.len());
    assert_eq!(output.stats.copied_subtrees, 2);
}

#[test]
fn malformed_lexical_and_structural_inputs_never_take_an_unvalidated_copy_path() {
    let cases = [
        "",
        " ",
        "tru",
        "True",
        "null false",
        "[1,]",
        "{\"a\":1,}",
        "{\"a\" 1}",
        "[1 2]",
        "{a:1}",
        "{\"a\":}",
        "[",
        "{",
        "\"unterminated",
        "\"\\q\"",
        "\"\\u12\"",
        "\"\\uZZZZ\"",
        "\"\\uD800\"",
        "\"\\uDC00\"",
        "\"\\uD800\\u0041\"",
        "\"raw\nnewline\"",
        "01",
        "-",
        "1.",
        "1e",
        "1e+",
        "1e309",
        "+1",
        ".1",
        "NaN",
        "{\"unrelated\":[1,]}",
        "[true false]",
        "{}x",
        "\u{feff}null",
        "null\u{a0}",
    ];
    for input in cases {
        assert!(
            serde_json::from_str::<Value>(input).is_err(),
            "oracle accepted {input:?}"
        );
        assert!(
            matches!(
                apply_rules_bytes(input.as_bytes(), &Trie::new(), &StreamingLimits::default()),
                Err(StreamingError::InvalidJson { .. })
            ),
            "accepted {input:?}"
        );
    }
    assert!(matches!(
        apply_rules_bytes(
            &[b'"', 0xff, b'"'],
            &Trie::new(),
            &StreamingLimits::default()
        ),
        Err(StreamingError::InvalidJson { .. })
    ));
}

#[test]
fn valid_primitive_encodings_and_empty_collections_copy_exactly() {
    for input in [
        "null",
        " true \t\r\n",
        "false",
        "[]",
        "{}",
        "[{},[]]",
        "-0",
        "-0.125e+02",
        "18446744073709551615",
        "-9223372036854775808",
        "1e-400",
        "\"\"",
        "\"\\/\\b\\f\\n\\r\\t\"",
        "\"\\u0000\\uD83E\\uDD80\"",
        "\"🦀é\"",
    ] {
        compare(input.as_bytes(), &Trie::new());
        assert_eq!(
            apply_rules_bytes(input.as_bytes(), &Trie::new(), &StreamingLimits::default())
                .unwrap()
                .bytes,
            input.as_bytes()
        );
    }
}

#[test]
fn input_output_depth_token_and_string_limits_have_explicit_boundaries() {
    let defaults = StreamingLimits::default();
    let empty = Trie::new();
    let check_limit = |input: &[u8], limits: StreamingLimits, label| {
        assert_eq!(
            apply_rules_bytes(input, &empty, &limits).unwrap_err(),
            StreamingError::LimitExceeded { limit: label }
        );
    };
    check_limit(
        b"null",
        StreamingLimits {
            max_input_bytes: 3,
            ..defaults
        },
        "input bytes",
    );
    check_limit(
        b"null",
        StreamingLimits {
            max_output_bytes: 3,
            ..defaults
        },
        "output bytes",
    );
    check_limit(
        b"[[0]]",
        StreamingLimits {
            max_depth: 1,
            ..defaults
        },
        "depth",
    );
    check_limit(
        br#"{"a":0}"#,
        StreamingLimits {
            max_tokens: 2,
            ..defaults
        },
        "tokens",
    );
    check_limit(
        br#""abc""#,
        StreamingLimits {
            max_string_bytes: 4,
            ..defaults
        },
        "string bytes",
    );
    assert!(
        apply_rules_bytes(
            b"null",
            &empty,
            &StreamingLimits {
                max_input_bytes: 4,
                max_output_bytes: 4,
                max_depth: 0,
                max_tokens: 1,
                ..defaults
            }
        )
        .is_ok()
    );
    assert!(
        apply_rules_bytes(
            br#"{"a":0}"#,
            &empty,
            &StreamingLimits {
                max_tokens: 3,
                max_depth: 1,
                ..defaults
            }
        )
        .is_ok()
    );
    assert!(
        apply_rules_bytes(
            br#""abc""#,
            &empty,
            &StreamingLimits {
                max_string_bytes: 5,
                ..defaults
            }
        )
        .is_ok()
    );
    assert_eq!(
        apply_rules_bytes(
            b"null",
            &empty,
            &StreamingLimits {
                max_depth: 129,
                ..defaults
            }
        )
        .unwrap_err(),
        StreamingError::InvalidLimits
    );
    let deep = format!("{}null{}", "[".repeat(129), "]".repeat(129));
    check_limit(
        deep.as_bytes(),
        StreamingLimits {
            max_depth: 128,
            ..defaults
        },
        "depth",
    );
}

#[test]
fn output_growth_and_repeated_replacement_allocations_are_bounded() {
    let trie = rules("", "a", "longer");
    assert_eq!(
        apply_rules_bytes(
            br#"{"a":0}"#,
            &trie,
            &StreamingLimits {
                max_output_bytes: 11,
                ..StreamingLimits::default()
            }
        )
        .unwrap_err(),
        StreamingError::LimitExceeded {
            limit: "output bytes"
        }
    );
    assert!(
        apply_rules_bytes(
            br#"{"a":0}"#,
            &trie,
            &StreamingLimits {
                max_output_bytes: 12,
                ..StreamingLimits::default()
            }
        )
        .is_ok()
    );
    assert_eq!(
        apply_rules_bytes(
            br#"{"a":0}"#,
            &trie,
            &StreamingLimits {
                max_string_bytes: 7,
                ..StreamingLimits::default()
            }
        )
        .unwrap_err(),
        StreamingError::LimitExceeded {
            limit: "string bytes"
        }
    );
    let mut trie = rules("/one", "a", &"x".repeat(100));
    trie.insert(
        &Pointer::parse("/two").unwrap(),
        Rename::new("a", "x".repeat(100)),
    );
    assert_eq!(
        apply_rules_bytes(
            br#"{"one":{"a":0},"two":{"a":0}}"#,
            &trie,
            &StreamingLimits {
                max_output_bytes: 150,
                ..StreamingLimits::default()
            }
        )
        .unwrap_err(),
        StreamingError::LimitExceeded {
            limit: "output bytes"
        }
    );
}

#[test]
fn array_rules_remain_rejected_but_numeric_object_keys_work() {
    let trie = rules("/items/0", "old", "new");
    compare(br#"{"items":[{"old":1}]}"#, &trie);
    compare(br#"{"items":{"0":{"old":1}},"other":[{"old":2}]}"#, &trie);
}

#[test]
fn deterministic_generated_documents_differentially_match_the_reference() {
    // A fixed LCG seed generates nested fixtures without a new test dependency.
    fn generate(seed: &mut u64, depth: usize) -> Value {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        if depth == 0 {
            return json!(*seed % 1000);
        }
        match (*seed >> 32) % 6 {
            0 => Value::Null,
            1 => json!("quote\" back\\ newline\n 🦀"),
            2 => json!([generate(seed, depth - 1), generate(seed, depth - 1)]),
            3 => json!({"name":generate(seed, depth - 1),"keep":generate(seed, depth - 1)}),
            4 => json!(false),
            _ => json!({"title": generate(seed, depth - 1), "child": generate(seed, depth - 1)}),
        }
    }
    let mut seed = 0xB1F2057;
    let mut registry = Registry::builtin().unwrap();
    for _ in 0..200 {
        let input = json!({"session":{"terminal":{"name":generate(&mut seed, 5)}}, "other":generate(&mut seed, 5)});
        let bytes = serde_json::to_vec_pretty(&input).unwrap();
        for (from, to) in [(Revision::V1, Revision::V4), (Revision::V4, Revision::V1)] {
            let plan = registry.compile(from, to).unwrap();
            let streamed = plan
                .translate_bytes(&bytes, &StreamingLimits::default())
                .unwrap();
            assert_eq!(
                serde_json::from_slice::<Value>(&streamed.bytes).unwrap(),
                plan.translate(&input).unwrap()
            );
        }
    }
}

#[test]
fn byte_mutations_and_truncations_agree_with_serde_syntax_validation() {
    let fixtures: [&[u8]; 5] = [
        br#" {"a":[true,false,null,-1.25e+2],"s":"quote\" slash\\ \u0061"} "#,
        br#"[{},[],"\uD83E\uDD80",0,-0,1e-12]"#,
        br#"{"a":1,"b":{"x":2}}"#,
        b"null",
        br#""text""#,
    ];
    let empty = Trie::new();
    let check = |bytes: &[u8]| {
        let reference = serde_json::from_slice::<Value>(bytes);
        match apply_rules_bytes(bytes, &empty, &StreamingLimits::default()) {
            Ok(result) => {
                assert!(
                    reference.is_ok(),
                    "scanner accepted invalid bytes {bytes:?}"
                );
                assert_eq!(result.bytes, bytes);
            }
            Err(StreamingError::DuplicateKey { .. }) => {
                // The byte API deliberately rejects duplicates that Value collapses.
                assert!(reference.is_ok());
            }
            Err(StreamingError::InvalidJson { .. }) => {
                assert!(reference.is_err(), "scanner rejected valid bytes {bytes:?}");
            }
            Err(error) => panic!("unexpected error: {error}"),
        }
    };
    for fixture in fixtures {
        for end in 0..=fixture.len() {
            check(&fixture[..end]);
        }
        for index in 0..fixture.len() {
            for replacement in [
                b'{', b'}', b'[', b']', b',', b':', b'"', b'\\', b'0', b'9', b' ', b'\n', 0, 0xff,
            ] {
                let mut mutated = fixture.to_vec();
                mutated[index] = replacement;
                check(&mutated);
            }
        }
    }
}

#[test]
fn same_pass_descendant_edits_precede_parent_renames_and_paths_are_escaped() {
    let mut trie = rules("", "a/b", "renamed");
    trie.insert(
        &Pointer::parse("/a~1b/~0").unwrap(),
        Rename::new("old", "new"),
    );
    for input in [
        br#"{"a/b":{"~":{"old":1}}}"#.as_slice(),
        br#"{"a/b":{"~":{"old":1,"new":2}}}"#,
        br#"{"a/b":{"~":false}}"#,
    ] {
        compare(input, &trie);
    }
}
