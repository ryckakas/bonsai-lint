//! Conformance against the cognitive complexity scoring rules.

mod common;

use common::{assert_scores, findings};

#[test]
fn operator_sequences_cost_per_run_not_per_operator() {
    assert_scores(&[
        ("if (a && b && c) { f(); }", 2),
        ("if (a && b || c) { f(); }", 3),
        ("if (a && b && c || d || e && g) { f(); }", 4),
    ]);
}

#[test]
fn parentheses_do_not_break_an_operator_run() {
    assert_scores(&[
        ("if (a && (b && c)) { f(); }", 2),
        ("if (a && (b || c)) { f(); }", 3),
        ("if (a && (b || c) || d) { f(); }", 3),
        ("if (a && (b || c) && d) { f(); }", 4),
    ]);
}

#[test]
fn negation_starts_a_new_sequence() {
    assert_scores(&[("if (a && !(b && c)) { f(); }", 3)]);
}

/// Correct by omission: `??` is absent from the operator normaliser. Other implementations score
/// it, so without this test someone could "fix" it to match and silently move every score.
#[test]
fn null_coalescing_scores_zero() {
    assert_scores(&[
        ("if (a ?? b) { f(); }", 1),
        ("const x = a ?? b ?? c; return x;", 0),
    ]);
}

/// Correct by omission: `augmented_assignment_expression` is absent from the logical kinds, so
/// short-circuiting assignment is shorthand and costs nothing.
#[test]
fn logical_assignment_scores_zero() {
    assert_scores(&[("let x = 1; x ||= 2; x &&= 3; x ??= 4; return x;", 0)]);
}

#[test]
fn nesting_compounds() {
    assert_scores(&[
        ("if (a) { f(); }", 1),
        ("if (a) { if (b) { f(); } }", 3),
        ("if (a) { if (b) { if (c) { f(); } } }", 6),
        ("for (const x of xs) { if (x) { f(); } }", 3),
        ("for (const k in o) { if (k) { f(); } }", 3),
    ]);
}

/// TypeScript has no else-if clause, so every chain link arrives as a nested `if_statement`
/// inside an `else_clause` and must still take the flat increment.
#[test]
fn else_branches_take_a_flat_increment() {
    assert_scores(&[
        ("if (a) { f(); } else if (b) { g(); } else { h(); }", 3),
        ("if (a) { f(); } else if (b) { g(); }", 2),
        ("if (a) { f(); } else { g(); }", 2),
    ]);
}

#[test]
fn switch_costs_one_regardless_of_arm_count() {
    assert_scores(&[(
        "switch (a) { case 1: f(); break; case 2: g(); break; default: h(); }",
        1,
    )]);
}

#[test]
fn try_is_free_and_each_catch_costs_one() {
    assert_scores(&[
        ("try { f(); } catch (e) { g(); }", 1),
        ("try { f(); } catch (e) { if (a) { g(); } }", 3),
        ("try { f(); } finally { g(); }", 0),
    ]);
}

#[test]
fn labelled_jumps_cost_one_and_plain_jumps_are_free() {
    assert_scores(&[
        ("for (const x of xs) { break; }", 1),
        (
            "outer: for (const x of xs) { for (const y of x) { break outer; } }",
            4,
        ),
        (
            "outer: for (const x of xs) { for (const y of x) { continue outer; } }",
            4,
        ),
    ]);
}

/// A lambda scores +0 but raises the nesting level, and the total is attributed to the
/// enclosing unit.
#[test]
fn closures_raise_nesting_without_scoring() {
    assert_scores(&[
        ("const r = () => { if (a) { f(); } };", 2),
        ("if (x) { const r = () => { if (a) { f(); } }; }", 4),
    ]);
}

/// A unit scores its body, so its own defaults are scored nowhere, while a nested function's are
/// part of the enclosing unit. Where a default belongs waits on one decision across every language.
#[test]
fn only_a_nested_functions_defaults_are_scored() {
    let own = findings("function f(x = a ? 1 : 2) {}");
    let f = own.iter().find(|finding| finding.name == "f");
    assert_eq!(f.map(|finding| finding.score), Some(0));
    assert!(own
        .iter()
        .all(|finding| finding.name != bonsai_core::TOPLEVEL_UNIT));

    assert_scores(&[("function f(x = a ? 1 : 2) {}", 2)]);
}

/// Callback pyramids are the way JavaScript becomes unreadable. Scoring each function from zero
/// would report this as two easy functions instead of one hard one.
#[test]
fn callback_nesting_compounds_into_the_enclosing_unit() {
    assert_scores(&[(
        "if (a) { for (const x of xs) { xs.forEach(item => { if (b) { f(); } }); } }",
        7,
    )]);
}

#[test]
fn direct_recursion_costs_one() {
    assert_scores(&[
        ("return target();", 1),
        ("return this.target();", 1),
        ("return other.target();", 0),
        ("return unrelated();", 0),
    ]);
}

fn score_of_target(source: &str) -> u32 {
    findings(source)
        .into_iter()
        .find(|finding| finding.name == "target")
        .unwrap_or_else(|| panic!("no finding produced for:\n{source}"))
        .score
}

#[test]
fn a_method_recurses_through_this_super_or_its_container() {
    for (source, expected) in [
        ("class C { target() { return this.target(); } }", 1),
        ("class C { target() { return super.target(); } }", 1),
        ("class C { static target() { return C.target(); } }", 1),
        ("class C { target() { return D.target(); } }", 0),
        (
            "namespace N { class C { target() { return C.target(); } } }",
            1,
        ),
        (
            "namespace N { class C { target() { return N.target(); } } }",
            0,
        ),
        (
            "const api = { users: { target() { return users.target(); } } };",
            1,
        ),
        (
            "const api = { users: { target() { return api.target(); } } };",
            0,
        ),
    ] {
        assert_eq!(score_of_target(source), expected, "scoring:\n{source}");
    }
}

/// The receiver is matched against the last `::` segment of the container path, so a quoted
/// key that itself contains `::` answers to its tail.
#[test]
fn a_quoted_key_with_a_separator_answers_to_its_last_segment() {
    assert_eq!(
        score_of_target(r#"const o = { "a::b": { target() { return b.target(); } } };"#),
        1
    );
}

/// Arguably wrong, and pinned so that changing it is a deliberate scoring decision.
#[test]
fn a_bare_namesake_call_inside_a_method_counts() {
    assert_eq!(
        score_of_target("class C { target() { return target(); } }"),
        1
    );
}

/// Arguably wrong, and pinned so that changing it is a deliberate scoring decision.
#[test]
fn this_counts_as_self_even_in_a_free_function() {
    assert_scores(&[("return this.target();", 1), ("return super.target();", 1)]);
}

/// A limit, not a choice: without scope analysis a closure that shadows its function's name
/// reads as a self-call, one increment per call.
#[test]
fn a_local_closure_sharing_the_functions_name_counts_as_recursion() {
    assert_scores(&[("const target = () => 1; target(); target();", 2)]);
}

#[test]
fn a_linear_function_scores_zero() {
    assert_scores(&[("const a = 1; const b = 2; return a + b;", 0)]);
}
