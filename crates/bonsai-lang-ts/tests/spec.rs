//! Conformance against the `SonarSource` cognitive complexity specification.

mod common;

use common::assert_scores;

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
    ]);
}

#[test]
fn negation_starts_a_new_sequence() {
    assert_scores(&[("if (a && !(b && c)) { f(); }", 3)]);
}

/// Correct by omission: `??` is absent from the operator normaliser. The reference JavaScript
/// analyser scores it, so without this test someone could "fix" it to match and silently move
/// every score.
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

/// The specification's own worked example: a lambda scores +0 but raises the nesting level, and
/// the total is attributed to the enclosing unit.
#[test]
fn closures_raise_nesting_without_scoring() {
    assert_scores(&[
        ("const r = () => { if (a) { f(); } };", 2),
        ("if (x) { const r = () => { if (a) { f(); } }; }", 4),
    ]);
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

#[test]
fn a_linear_function_scores_zero() {
    assert_scores(&[("const a = 1; const b = 2; return a + b;", 0)]);
}
