use std::sync::Arc;

use bifrost::{
    BifrostError, CacheStats, Pointer, Registry, Rename, Revision, Trie, apply_rules, translate,
};
use serde_json::{Value, json};

const REVISIONS: [Revision; 4] = [Revision::V1, Revision::V2, Revision::V3, Revision::V4];
const FIELDS: [&str; 4] = ["name", "title", "label", "display_name"];

fn step(path: &str, source: &str, destination: &str) -> Trie<Rename> {
    let mut rules = Trie::new();
    rules.insert(
        &Pointer::parse(path).unwrap(),
        Rename::new(source, destination),
    );
    rules
}

#[test]
fn all_builtin_pairs_match_sequential_reference_and_expected_values() {
    let mut registry = Registry::builtin().unwrap();
    for (source_index, &from) in REVISIONS.iter().enumerate() {
        for (target_index, &to) in REVISIONS.iter().enumerate() {
            for value in [
                Value::Null,
                json!("astrid 🦀 \n \"quoted\""),
                json!([1, true, {"nested": false}]),
            ] {
                let input = json!({
                    "session": {"terminal": {FIELDS[source_index]: value.clone(), "status": "running"}},
                    "metadata": {"name": [null, {"title": "unchanged"}]}
                });
                let original = input.clone();
                let mut reference = input.clone();
                let mut current = source_index;
                while current != target_index {
                    let next = if current < target_index {
                        current + 1
                    } else {
                        current - 1
                    };
                    reference = apply_rules(
                        &reference,
                        &step("/session/terminal", FIELDS[current], FIELDS[next]),
                    )
                    .unwrap();
                    current = next;
                }
                let plan = registry.compile(from, to).unwrap();
                assert_eq!(plan.source(), from);
                assert_eq!(plan.target(), to);
                assert_eq!(plan.hop_count(), source_index.abs_diff(target_index));
                assert_eq!(plan.step_count(), plan.hop_count());
                let output = plan.translate(&input).unwrap();
                assert_eq!(output, reference);
                assert_eq!(
                    output,
                    json!({
                        "session": {"terminal": {FIELDS[target_index]: value, "status": "running"}},
                        "metadata": {"name": [null, {"title": "unchanged"}]}
                    })
                );
                assert_eq!(registry.translate(&output, to, from).unwrap(), input);
                assert_eq!(translate(&input, from, to).unwrap(), output);
                assert_eq!(input, original);
            }
        }
    }
}

#[test]
fn lazy_compilation_caches_only_requested_pairs_and_reuses_the_same_plan() {
    let mut registry = Registry::builtin().unwrap();
    assert_eq!(
        registry.cache_stats(),
        CacheStats {
            compilations: 0,
            cached_chains: 0
        }
    );
    let first = registry.compile(Revision::V1, Revision::V4).unwrap();
    assert_eq!(
        registry.cache_stats(),
        CacheStats {
            compilations: 1,
            cached_chains: 1
        }
    );
    let repeated = registry.compile(Revision::V1, Revision::V4).unwrap();
    assert!(Arc::ptr_eq(&first, &repeated));
    registry
        .translate(&json!({}), Revision::V1, Revision::V4)
        .unwrap();
    assert_eq!(
        registry.cache_stats(),
        CacheStats {
            compilations: 1,
            cached_chains: 1
        }
    );
    let reverse = registry.compile(Revision::V4, Revision::V1).unwrap();
    assert!(!Arc::ptr_eq(&first, &reverse));
    assert_eq!(
        registry.cache_stats(),
        CacheStats {
            compilations: 2,
            cached_chains: 2
        }
    );
}

#[test]
fn identity_plans_are_empty_and_return_independent_values() {
    let mut registry = Registry::builtin().unwrap();
    for revision in REVISIONS {
        let plan = registry.compile(revision, revision).unwrap();
        assert_eq!((plan.hop_count(), plan.step_count()), (0, 0));
        for input in [
            Value::Null,
            json!([1]),
            json!({"session": false}),
            json!({"session": {"terminal": {"name": 1, "title": 2}}}),
        ] {
            assert_eq!(plan.translate(&input), Ok(input));
        }
        let input = json!({"nested": [1]});
        let mut output = plan.translate(&input).unwrap();
        output["nested"][0] = json!(2);
        assert_eq!(input, json!({"nested": [1]}));
    }
}

#[test]
fn explicit_direction_and_empty_transition_are_distinct_from_missing_transition() {
    let mut registry = Registry::new([Revision::V2, Revision::V1]);
    registry
        .set_transition(Revision::V1, Revision::V2, vec![])
        .unwrap();
    let plan = registry.compile(Revision::V1, Revision::V2).unwrap();
    assert_eq!((plan.hop_count(), plan.step_count()), (1, 0));
    assert_eq!(plan.translate(&Value::Null), Ok(Value::Null));
    assert_eq!(
        registry.compile(Revision::V2, Revision::V1).unwrap_err(),
        BifrostError::MissingTransition {
            from: Revision::V2,
            to: Revision::V1
        }
    );
    assert_eq!(
        registry.cache_stats(),
        CacheStats {
            compilations: 1,
            cached_chains: 1
        }
    );
}

#[test]
fn unknown_numbers_endpoints_and_intermediate_revisions_are_rejected() {
    for number in [0, 5, u32::MAX] {
        assert_eq!(
            Revision::try_from(number),
            Err(BifrostError::UnknownRevision { revision: number })
        );
    }
    for revision in REVISIONS {
        assert_eq!(Revision::try_from(revision.number()).unwrap(), revision);
    }
    let mut registry = Registry::new([Revision::V1, Revision::V3]);
    assert_eq!(
        registry.compile(Revision::V1, Revision::V3).unwrap_err(),
        BifrostError::UnknownRevision { revision: 2 }
    );
    assert_eq!(
        registry.compile(Revision::V4, Revision::V4).unwrap_err(),
        BifrostError::UnknownRevision { revision: 4 }
    );
    assert_eq!(
        registry.compile(Revision::V1, Revision::V4).unwrap_err(),
        BifrostError::UnknownRevision { revision: 4 }
    );
    assert_eq!(
        registry.cache_stats(),
        CacheStats {
            compilations: 0,
            cached_chains: 0
        }
    );
}

#[test]
fn a_missing_middle_transition_reports_the_exact_failed_hop() {
    let mut registry = Registry::new(REVISIONS);
    registry
        .set_transition(Revision::V1, Revision::V2, vec![])
        .unwrap();
    registry
        .set_transition(Revision::V3, Revision::V4, vec![])
        .unwrap();
    assert_eq!(
        registry.compile(Revision::V1, Revision::V4).unwrap_err(),
        BifrostError::MissingTransition {
            from: Revision::V2,
            to: Revision::V3
        }
    );
    assert_eq!(
        registry.cache_stats(),
        CacheStats {
            compilations: 0,
            cached_chains: 0
        }
    );
    registry
        .set_transition(Revision::V2, Revision::V3, vec![])
        .unwrap();
    assert_eq!(
        registry
            .compile(Revision::V1, Revision::V4)
            .unwrap()
            .hop_count(),
        3
    );
}

#[test]
fn ordered_steps_and_revision_boundaries_preserve_successive_renames() {
    let mut registry = Registry::new(REVISIONS);
    let first_steps = vec![step("", "a", "b"), step("/b", "old", "middle")];
    let second_steps = vec![step("/b", "middle", "new")];
    let input = json!({"a": {"old": 42}, "keep": [null]});
    let mut reference = input.clone();
    for rules in first_steps.iter().chain(&second_steps) {
        reference = apply_rules(&reference, rules).unwrap();
    }
    registry
        .set_transition(Revision::V1, Revision::V2, first_steps)
        .unwrap();
    registry
        .set_transition(Revision::V2, Revision::V3, second_steps)
        .unwrap();
    let plan = registry.compile(Revision::V1, Revision::V3).unwrap();
    assert_eq!((plan.hop_count(), plan.step_count()), (2, 3));
    assert_eq!(plan.translate(&input).unwrap(), reference);
    assert_eq!(reference, json!({"b": {"new": 42}, "keep": [null]}));
}

#[test]
fn intermediate_collisions_are_not_bypassed_and_errors_preserve_input() {
    let mut registry = Registry::builtin().unwrap();
    for (input, from, to, source, destination) in [
        (
            json!({"session": {"terminal": {"name": "a", "title": "b"}}}),
            Revision::V1,
            Revision::V4,
            "name",
            "title",
        ),
        (
            json!({"session": {"terminal": {"name": "a", "label": "b"}}}),
            Revision::V1,
            Revision::V4,
            "title",
            "label",
        ),
        (
            json!({"session": {"terminal": {"display_name": "a", "title": "b"}}}),
            Revision::V4,
            Revision::V1,
            "label",
            "title",
        ),
    ] {
        let original = input.clone();
        assert_eq!(
            registry.translate(&input, from, to),
            Err(BifrostError::Conflict {
                path: "/session/terminal".into(),
                source: source.into(),
                destination: destination.into(),
            })
        );
        assert_eq!(input, original);
    }
}

#[test]
fn later_schema_error_matches_sequential_execution_without_mutating_input() {
    let mut registry = Registry::new(REVISIONS);
    registry
        .set_transition(Revision::V1, Revision::V2, vec![step("", "old", "new")])
        .unwrap();
    registry
        .set_transition(Revision::V2, Revision::V3, vec![step("/new", "a", "b")])
        .unwrap();
    let input = json!({"old": false});
    let original = input.clone();
    let intermediate = apply_rules(&input, &step("", "old", "new")).unwrap();
    let reference = apply_rules(&intermediate, &step("/new", "a", "b"));
    assert_eq!(
        registry.translate(&input, Revision::V1, Revision::V3),
        reference
    );
    assert_eq!(
        reference,
        Err(BifrostError::ExpectedObject {
            path: "/new".into()
        })
    );
    assert_eq!(input, original);
}

#[test]
fn mutation_invalidates_every_cached_pair_while_old_handles_remain_snapshots() {
    let mut registry = Registry::builtin().unwrap();
    let old = registry.compile(Revision::V1, Revision::V4).unwrap();
    registry.compile(Revision::V4, Revision::V1).unwrap();
    registry.compile(Revision::V1, Revision::V1).unwrap();
    registry
        .set_transition(
            Revision::V2,
            Revision::V3,
            vec![step("/session/terminal", "title", "caption")],
        )
        .unwrap();
    assert_eq!(
        registry.cache_stats(),
        CacheStats {
            compilations: 3,
            cached_chains: 0
        }
    );
    let updated = registry.compile(Revision::V1, Revision::V4).unwrap();
    assert!(!Arc::ptr_eq(&old, &updated));
    let input = json!({"session": {"terminal": {"name": "astrid"}}});
    assert_eq!(
        old.translate(&input).unwrap(),
        json!({"session": {"terminal": {"display_name": "astrid"}}})
    );
    assert_eq!(
        updated.translate(&input).unwrap(),
        json!({"session": {"terminal": {"caption": "astrid"}}})
    );
    assert!(Arc::ptr_eq(
        &updated,
        &registry.compile(Revision::V1, Revision::V4).unwrap()
    ));
    assert_eq!(
        registry.cache_stats(),
        CacheStats {
            compilations: 4,
            cached_chains: 1
        }
    );
}

#[test]
fn failed_updates_do_not_invalidate_cache_and_nonadjacent_hops_are_rejected() {
    let mut registry = Registry::new([Revision::V1, Revision::V2, Revision::V3]);
    registry
        .set_transition(Revision::V1, Revision::V2, vec![])
        .unwrap();
    let plan = registry.compile(Revision::V1, Revision::V2).unwrap();
    for (from, to, expected) in [
        (
            Revision::V1,
            Revision::V3,
            BifrostError::NonAdjacentTransition {
                from: Revision::V1,
                to: Revision::V3,
            },
        ),
        (
            Revision::V2,
            Revision::V2,
            BifrostError::NonAdjacentTransition {
                from: Revision::V2,
                to: Revision::V2,
            },
        ),
        (
            Revision::V3,
            Revision::V4,
            BifrostError::UnknownRevision { revision: 4 },
        ),
    ] {
        assert_eq!(registry.set_transition(from, to, vec![]), Err(expected));
    }
    assert!(Arc::ptr_eq(
        &plan,
        &registry.compile(Revision::V1, Revision::V2).unwrap()
    ));
    assert_eq!(
        registry.cache_stats(),
        CacheStats {
            compilations: 1,
            cached_chains: 1
        }
    );
}

#[test]
fn separate_registries_never_share_cached_configuration() {
    let mut first = Registry::builtin().unwrap();
    let mut second = Registry::builtin().unwrap();
    second
        .set_transition(
            Revision::V1,
            Revision::V2,
            vec![step("/session/terminal", "name", "custom")],
        )
        .unwrap();
    let first_plan = first.compile(Revision::V1, Revision::V2).unwrap();
    let second_plan = second.compile(Revision::V1, Revision::V2).unwrap();
    assert!(!Arc::ptr_eq(&first_plan, &second_plan));
    let input = json!({"session": {"terminal": {"name": "astrid"}}});
    assert_ne!(
        first_plan.translate(&input).unwrap(),
        second_plan.translate(&input).unwrap()
    );
}
