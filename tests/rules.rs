use bifrost::{BifrostError, Pointer, Rename, Trie, apply_rules};
use serde_json::{Value, json};

fn add(trie: &mut Trie<Rename>, path: &str, from: &str, to: &str) {
    assert_eq!(
        trie.insert(&Pointer::parse(path).unwrap(), Rename::new(from, to)),
        None
    );
}

#[test]
fn generic_rules_handle_root_empty_keys_and_escaping() {
    let mut rules = Trie::new();
    add(&mut rules, "", "top", "renamed");
    add(&mut rules, "/", "", "empty-key");
    add(&mut rules, "/a~1b/~0", "old/key", "new~key");
    let input = json!({"top": true, "": {"": null}, "a/b": {"~": {"old/key": [1, 2]}}});
    assert_eq!(
        apply_rules(&input, &rules).unwrap(),
        json!({"renamed": true, "": {"empty-key": null}, "a/b": {"~": {"new~key": [1, 2]}}})
    );
}

#[test]
fn only_rule_bearing_paths_are_validated_and_missing_paths_are_skipped() {
    let mut rules = Trie::new();
    add(&mut rules, "/relevant/target", "old", "new");
    let input = json!({"relevant": {}, "unrelated": [null, false, 42]});
    assert_eq!(apply_rules(&input, &rules), Ok(input.clone()));
    assert_eq!(apply_rules(&json!({}), &rules), Ok(json!({})));
    let input = json!({"relevant": {"target": {"old": 1}}, "unrelated": null});
    assert_eq!(
        apply_rules(&input, &rules),
        Ok(json!({"relevant": {"target": {"new": 1}}, "unrelated": null}))
    );
}

#[test]
fn empty_trie_accepts_any_value_and_returns_an_independent_clone() {
    for input in [Value::Null, json!([1]), json!({"items": [1]})] {
        let mut output = apply_rules(&input, &Trie::new()).unwrap();
        assert_eq!(input, output);
        if output.is_object() {
            output["items"][0] = json!(2);
            assert_eq!(input, json!({"items": [1]}));
        }
    }
}

#[test]
fn array_paths_tokenize_but_applying_through_arrays_is_rejected() {
    let mut rules = Trie::new();
    add(&mut rules, "/items/0", "old", "new");
    assert_eq!(
        apply_rules(&json!({"items": [{"old": 1}]}), &rules),
        Err(BifrostError::ExpectedObject {
            path: "/items".into()
        })
    );
    // The same token is a valid object key; tokenization does not choose its type.
    assert_eq!(
        apply_rules(&json!({"items": {"0": {"old": 1}}}), &rules),
        Ok(json!({"items": {"0": {"new": 1}}}))
    );
}

#[test]
fn dynamic_error_paths_are_reencoded_and_input_is_preserved_on_failure() {
    let mut rules = Trie::new();
    add(&mut rules, "/a", "old", "new");
    add(&mut rules, "/z~1x/~0", "old", "new");
    let input = json!({"a": {"old": 1}, "z/x": {"~": {"old": null, "new": null}}});
    let original = input.clone();
    assert_eq!(
        apply_rules(&input, &rules),
        Err(BifrostError::Conflict {
            path: "/z~1x/~0".into(),
            source: "old".into(),
            destination: "new".into(),
        })
    );
    assert_eq!(input, original);
    assert_eq!(
        apply_rules(&json!({"z/x": {"~": false}}), &rules),
        Err(BifrostError::ExpectedObject {
            path: "/z~1x/~0".into()
        })
    );
}

#[test]
fn descendants_run_before_parent_renames_regardless_of_insertion_order() {
    for parent_first in [true, false] {
        let mut rules = Trie::new();
        let mut entries = vec![("", "a", "b"), ("/a", "old", "new")];
        if !parent_first {
            entries.reverse();
        }
        for (path, from, to) in entries {
            add(&mut rules, path, from, to);
        }
        assert_eq!(
            apply_rules(&json!({"a": {"old": 1}}), &rules),
            Ok(json!({"b": {"new": 1}}))
        );
    }
}

#[test]
fn sibling_error_order_is_deterministic_and_equal_names_are_noop() {
    let mut rules = Trie::new();
    add(&mut rules, "/z", "old", "new");
    add(&mut rules, "/a", "old", "new");
    assert_eq!(
        apply_rules(&json!({"z": null, "a": null}), &rules),
        Err(BifrostError::ExpectedObject { path: "/a".into() })
    );
    let mut rules = Trie::new();
    add(&mut rules, "", "same", "same");
    let input = json!({"same": null});
    assert_eq!(apply_rules(&input, &rules), Ok(input));
}
