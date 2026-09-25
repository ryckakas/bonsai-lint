//! Conformance against the cognitive complexity scoring rules.

mod common;

use common::{assert_scores, score_of};

#[test]
fn operator_sequences_cost_per_run_not_per_operator() {
    assert_scores(&[
        ("if (a && b && c) { f(); }", 2),
        ("if (a && b || c) { f(); }", 3),
        ("if (a && b && c || d || e && g) { f(); }", 4),
        ("x = a && b;", 1),
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

/// On booleans `&`, `|` and `^` are logical, but nothing in the syntax tells them apart from the
/// bitwise operators, so neither is counted.
#[test]
fn non_short_circuit_operators_score_zero() {
    assert_scores(&[("x = a & b | c;", 0), ("x = a ^ b;", 0)]);
}

#[test]
fn nesting_compounds() {
    assert_scores(&[
        ("if (a) { f(); }", 1),
        ("if (a) { if (b) { f(); } }", 3),
        ("if (a) { if (b) { if (c) { f(); } } }", 6),
        ("for (int x : xs) { if (x > 0) { while (a) { f(); } } }", 6),
    ]);
}

#[test]
fn every_loop_form_costs_one_plus_nesting() {
    assert_scores(&[
        ("for (int i = 0; i < n; i++) { f(); }", 1),
        ("for (;;) { f(); }", 1),
        ("for (var x : xs) { f(); }", 1),
        ("while (a) { f(); }", 1),
        ("do { f(); } while (a);", 1),
        ("if (a) { while (b) { f(); } }", 3),
    ]);
}

/// `do … while (c)` puts its condition after the body, which must not make it nest.
#[test]
fn loop_headers_do_not_nest() {
    assert_scores(&[
        ("for (int i = 0; a && b; i++) { f(); }", 2),
        (
            "while (xs.stream().anyMatch(x -> { if (x) { return true; } return false; })) { f(); }",
            3,
        ),
        (
            "do { f(); } while (xs.stream().anyMatch(x -> x > 0 ? true : false));",
            3,
        ),
        (
            "for (var x : xs.stream().filter(y -> y > 0 ? true : false).toList()) { f(); }",
            3,
        ),
    ]);
}

/// Java has no else node, so a chain link arrives either as the next `if_statement` or as the
/// alternative statement itself, braced or not.
#[test]
fn else_branches_take_a_flat_increment() {
    assert_scores(&[
        ("if (a) { f(); } else if (b) { g(); } else { h(); }", 3),
        ("if (a) { f(); } else if (b) { g(); }", 2),
        ("if (a) { f(); } else { g(); }", 2),
        (
            "for (;;) { if (a) { f(); } else if (b) { g(); } else { h(); } }",
            5,
        ),
        ("if (a) { f(); } else if (b) { if (c) { g(); } }", 4),
    ]);
}

#[test]
fn unbraced_branches_score_like_braced_ones() {
    assert_scores(&[
        ("if (a) f(); else if (b) g(); else h();", 3),
        ("if (a) f(); else for (;;) { g(); }", 4),
    ]);
}

/// An `if` alone inside the else braces is nested code, not an else-if, because the whole else
/// block is walked one level deeper.
#[test]
fn an_if_alone_inside_else_braces_still_nests() {
    assert_scores(&[("if (a) { f(); } else { if (b) { g(); } }", 4)]);
}

#[test]
fn an_else_block_is_scored_whole() {
    assert_scores(&[("if (a) { f(); } else { g(); if (b) { h(); } }", 4)]);
}

/// Statement and expression switches, with classic groups or arrow rules, are one node kind.
#[test]
fn switch_costs_one_regardless_of_arm_count() {
    assert_scores(&[
        (
            "switch (a) { case 1: f(); break; case 2: g(); break; default: h(); }",
            1,
        ),
        (
            "switch (a) { case 1 -> f(); case 2 -> g(); default -> h(); }",
            1,
        ),
        (
            "int y = switch (a) { case 1 -> 1; default -> { yield 2; } };",
            1,
        ),
    ]);
}

#[test]
fn switch_cases_nest() {
    assert_scores(&[
        ("switch (a) { case 1: if (b) { f(); } }", 3),
        (
            "switch (a) { case 1 -> { if (b) { f(); } } default -> g(); }",
            3,
        ),
    ]);
}

/// A guard is part of the arm it selects: its operators count, the guard itself does not.
#[test]
fn a_guard_costs_only_its_operators() {
    assert_scores(&[(
        "switch (o) { case Foo x when a && b -> f(); default -> g(); }",
        2,
    )]);
}

#[test]
fn try_is_free_and_each_catch_costs_one() {
    assert_scores(&[
        ("try { f(); } catch (E e) { g(); }", 1),
        ("try { f(); } catch (E e) { if (a) { g(); } }", 3),
        ("try { f(); } finally { g(); }", 0),
        ("try { f(); } catch (A | B e) { g(); }", 1),
        ("try { f(); } catch (A e) { g(); } catch (B e) { h(); }", 2),
        ("try (var in = open()) { f(); }", 0),
        ("try (var in = open()) { f(); } catch (E e) { g(); }", 1),
    ]);
}

#[test]
fn ternaries_cost_one_plus_nesting() {
    assert_scores(&[("int y = a ? 1 : 2;", 1), ("int y = a ? 1 : b ? 2 : 3;", 3)]);
}

#[test]
fn labelled_jumps_cost_one_and_plain_jumps_are_free() {
    assert_scores(&[
        ("for (;;) { break; }", 1),
        ("for (;;) { continue; }", 1),
        (
            "outer:\nfor (var x : xs) {\n  for (var y : x) {\n    break outer;\n  }\n}",
            4,
        ),
        (
            "outer:\nfor (var x : xs) {\n  for (var y : x) {\n    continue outer;\n  }\n}",
            4,
        ),
    ]);
}

/// `synchronized` guards a block without branching, so it neither costs nor nests.
#[test]
fn synchronized_is_free_and_does_not_nest() {
    assert_scores(&[
        ("synchronized (lock) { f(); }", 0),
        ("synchronized (lock) { if (a) { f(); } }", 1),
    ]);
}

#[test]
fn statements_that_do_not_branch_score_zero() {
    assert_scores(&[
        ("assert a != null;", 0),
        ("throw new IllegalStateException();", 0),
        ("boolean y = o instanceof Foo;", 0),
        ("Function<A, B> g = this::convert;", 0),
        ("return;", 0),
    ]);
}

#[test]
fn closures_raise_nesting_without_scoring() {
    assert_scores(&[
        ("Runnable r = () -> { if (a) { f(); } };", 2),
        ("Runnable r = () -> f();", 0),
    ]);
}

#[test]
fn an_anonymous_class_method_nests_like_a_closure() {
    assert_scores(&[(
        "new Thread(new Runnable() { public void run() { if (a) { f(); } } });",
        2,
    )]);
}

#[test]
fn callback_nesting_compounds_into_the_enclosing_unit() {
    assert_scores(&[(
        "if (a) { for (var x : xs) { xs.forEach(i -> { if (b) { f(); } }); } }",
        7,
    )]);
}

#[test]
fn direct_recursion_costs_one() {
    assert_scores(&[
        ("target();", 1),
        ("this.target();", 1),
        ("T.target();", 1),
        ("T.this.target();", 1),
        ("other.target();", 0),
    ]);
}

/// `super.m()` inside `m()` hands off to the parent's method: the everyday override that
/// extends its parent is not recursion.
#[test]
fn a_call_to_the_parent_implementation_is_not_recursion() {
    assert_scores(&[("super.target();", 0), ("T.super.target();", 0)]);
}

#[test]
fn constructor_chaining_is_not_recursion() {
    let source = "class P {\n  P(int x) { this(x, 0); }\n  P(int x, int y) { super(); }\n}\n";
    assert_eq!(score_of(source, "P::P(int)"), 0);
    assert_eq!(score_of(source, "P::P(int, int)"), 0);
}

/// Overloads share a name, so only a call whose argument count fits can be the method itself.
#[test]
fn a_delegating_overload_is_not_recursion() {
    let source =
        "class C {\n  void target(int a) { target(a, 0); }\n  void target(int a, int b) {}\n}\n";
    assert_eq!(score_of(source, "C::target(int)"), 0);
}

/// Recursion is matched by syntax alone, so an overload taking as many arguments reads as a
/// call to itself.
#[test]
fn a_same_arity_overload_counts_as_recursion() {
    let source =
        "class C {\n  void target(int a) { target(\"x\"); }\n  void target(String s) {}\n}\n";
    assert_eq!(score_of(source, "C::target(int)"), 1);
}

#[test]
fn a_varargs_method_reaches_itself_with_enough_arguments() {
    let source = "class C {\n  void target(String... a) { target(\"x\", \"y\"); }\n  void other(String f, Object... a) { other(); }\n  void more(String f, Object... a) { more(\"x\"); }\n}\n";
    assert_eq!(score_of(source, "C::target(String...)"), 1);
    assert_eq!(score_of(source, "C::other(String, Object...)"), 0);
    assert_eq!(score_of(source, "C::more(String, Object...)"), 1);
}

#[test]
fn recursion_inside_a_lambda_still_reaches_the_method() {
    assert_scores(&[("xs.forEach(x -> target());", 1)]);
}

#[test]
fn a_static_initializer_is_scored_as_a_unit() {
    let source = "class C {\n  static {\n    for (var x : xs) { if (x) { f(); } }\n  }\n}\n";
    assert_eq!(score_of(source, "C::<static>"), 3);
}

#[test]
fn a_linear_method_scores_zero() {
    assert_scores(&[("int x = 1;\nf(x);\nreturn;", 0)]);
}
