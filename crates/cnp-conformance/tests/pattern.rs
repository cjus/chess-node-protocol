//! Patterns: what matches what.

use cnp_conformance::pattern::{Bindings, matches};
use serde_json::{Value, json};

fn ok(pattern: Value, actual: Value) -> bool {
    matches(&pattern, &actual, &mut Bindings::new()).is_ok()
}

#[test]
fn objects_need_the_pattern_s_keys_and_allow_others() {
    assert!(ok(json!({"a": 1}), json!({"a": 1, "b": 2})));
    assert!(!ok(json!({"a": 1, "c": 3}), json!({"a": 1, "b": 2})));
    assert!(!ok(json!({"a": 1}), json!([1])));
}

#[test]
fn arrays_match_element_by_element_at_the_same_length() {
    assert!(ok(json!([1, "$any"]), json!([1, {"x": 2}])));
    assert!(!ok(json!([1]), json!([1, 2])));
}

#[test]
fn numbers_compare_as_numbers() {
    assert!(ok(json!(1), json!(1.0)));
    assert!(ok(json!(-32800), json!(-32800)));
    assert!(!ok(json!(1), json!(2)));
}

#[test]
fn type_patterns_match_their_type() {
    for (pattern, good, bad) in [
        ("$string", json!("x"), json!(1)),
        ("$number", json!(1.5), json!("1")),
        ("$integer", json!(3), json!(3.5)),
        ("$boolean", json!(false), json!(0)),
        ("$array", json!([]), json!({})),
        ("$object", json!({}), json!([])),
        ("$null", json!(null), json!(0)),
        ("$move", json!("e7e8q"), json!("e7e8Q")),
        ("$any", json!(null), json!("never fails")),
    ] {
        assert!(ok(json!(pattern), good.clone()), "{pattern} {good}");
        if pattern != "$any" {
            assert!(!ok(json!(pattern), bad.clone()), "{pattern} {bad}");
        }
    }
}

#[test]
fn absent_matches_a_missing_or_null_field_only() {
    assert!(ok(json!({"score": "$absent"}), json!({"bestMove": null})));
    assert!(ok(json!({"score": "$absent"}), json!({"score": null})));
    assert!(!ok(
        json!({"score": "$absent"}),
        json!({"score": {"cp": 1}})
    ));
    assert!(!ok(json!({"score": "$any"}), json!({})));
}

#[test]
fn a_double_dollar_is_a_literal_dollar() {
    assert!(ok(json!("$$any"), json!("$any")));
    assert!(!ok(json!("$$any"), json!("anything")));
}

#[test]
fn bindings_are_kept_only_when_the_whole_pattern_matches() {
    let mut bindings = Bindings::new();
    // Binds, then fails further on: nothing is kept.
    let failed = matches(
        &json!({"a": "$bind:x", "b": 2}),
        &json!({"a": 1, "b": 3}),
        &mut bindings,
    );
    assert!(failed.is_err());
    assert!(bindings.is_empty());
    matches(&json!({"a": "$bind:x"}), &json!({"a": 1}), &mut bindings).unwrap();
    assert!(matches(&json!("$ref:x"), &json!(1), &mut bindings).is_ok());
    assert!(matches(&json!("$ref:x"), &json!(2), &mut bindings).is_err());
    assert!(matches(&json!("$ref:never"), &json!(2), &mut bindings).is_err());
}

#[test]
fn a_mismatch_says_where() {
    let mismatch = matches(
        &json!({"result": {"score": {"cp": 1}}}),
        &json!({"result": {"score": {"cp": 2}}}),
        &mut Bindings::new(),
    )
    .unwrap_err();
    assert_eq!(mismatch.path, "result.score.cp");
}

#[test]
fn an_unknown_dollar_pattern_is_an_error_not_a_match() {
    assert!(!ok(json!("$nonsense"), json!("$nonsense")));
}
