//! Conformance against the cognitive complexity scoring rules.

mod common;

use common::{assert_scores, findings};

#[test]
fn operator_sequences_cost_per_run_not_per_operator() {
    assert_scores(&[
        ("if a && b && c { f() }", 2),
        ("if a && b || c { f() }", 3),
        ("if a && b && c || d || e && g { f() }", 4),
    ]);
}

#[test]
fn parentheses_do_not_break_an_operator_run() {
    assert_scores(&[
        ("if a && (b && c) { f() }", 2),
        ("if a && (b || c) { f() }", 3),
        ("if a && (b || c) || d { f() }", 3),
    ]);
}

#[test]
fn negation_starts_a_new_sequence() {
    assert_scores(&[("if a && !(b && c) { f() }", 3)]);
}

#[test]
fn nesting_compounds() {
    assert_scores(&[
        ("if a { f() }", 1),
        ("if a { if b { f() } }", 3),
        ("if a { if b { if c { f() } } }", 6),
        ("for _, x := range xs { if x { f() } }", 3),
        ("for k := range m { if k != \"\" { f() } }", 3),
    ]);
}

/// Go spells every loop `for`; the four forms are one node kind and cost the same.
#[test]
fn every_for_form_costs_one_plus_nesting() {
    assert_scores(&[
        ("for i := 0; i < n; i++ { f() }", 1),
        ("for a { f() }", 1),
        ("for { f() }", 1),
        ("for range xs { f() }", 1),
        ("if a { for { f() } }", 3),
    ]);
}

#[test]
fn loop_headers_do_not_nest() {
    assert_scores(&[
        ("for i := 0; a && b; i++ { f() }", 2),
        (
            "for check(func() bool { if a { return true }; return false }) { f() }",
            3,
        ),
    ]);
}

/// Go has no else node, so a chain link arrives either as a bare `if_statement` or as the
/// alternative block itself, and must still take the flat increment.
#[test]
fn else_branches_take_a_flat_increment() {
    assert_scores(&[
        ("if a { f() } else if b { g() } else { h() }", 3),
        ("if a { f() } else if b { g() }", 2),
        ("if a { f() } else { g() }", 2),
        ("for { if a { f() } else if b { g() } else { h() } }", 5),
        ("if a { f() } else if b { if c { g() } }", 4),
        ("if a { f() } else { for { g() } }", 4),
    ]);
}

/// Pins the `statement_list` inside a 0.25 `block`: without it `walk_else` would take the lone
/// `if` for an else-if and score this 2.
#[test]
fn an_if_alone_inside_else_braces_still_nests() {
    assert_scores(&[("if a { f() } else { if b { g() } }", 4)]);
}

/// The other side of the same dependency: the whole else body is walked, not its first
/// statement.
#[test]
fn an_else_block_is_scored_whole() {
    assert_scores(&[("if a { f() } else { g(); if b { h() } }", 4)]);
}

#[test]
fn each_switch_kind_costs_one_regardless_of_case_count() {
    assert_scores(&[
        ("switch a { case 1: f(); case 2: g(); default: h() }", 1),
        (
            "switch v := x.(type) { case int: f(v); case string: g(v) }",
            1,
        ),
        (
            "select { case <-c: f(); case d <- 1: g(); default: h() }",
            1,
        ),
    ]);
}

#[test]
fn switch_cases_nest() {
    assert_scores(&[
        ("switch a { case 1: if b { f() } }", 3),
        ("select { case v := <-c: if v { f() } }", 3),
    ]);
}

/// `if x := f(); x` runs its initializer once, beside the condition, so neither nests.
#[test]
fn an_initializer_is_scored_as_header() {
    assert_scores(&[
        ("if ok := a && b; ok { f() }", 2),
        ("if err := target(); err != nil { f() }", 2),
        ("switch ok := a && b; ok { case true: f() }", 2),
        (
            "if check(func() bool { if a { return true }; return false }) { f() }",
            3,
        ),
        (
            "switch check(func() bool { if a { return true }; return false }) { case true: f() }",
            3,
        ),
    ]);
}

#[test]
fn labelled_jumps_cost_one_and_plain_jumps_are_free() {
    assert_scores(&[
        ("for { break }", 1),
        ("for { continue }", 1),
        (
            "outer:\n\tfor _, x := range xs {\n\t\tfor range x {\n\t\t\tbreak outer\n\t\t}\n\t}",
            4,
        ),
        (
            "outer:\n\tfor _, x := range xs {\n\t\tfor range x {\n\t\t\tcontinue outer\n\t\t}\n\t}",
            4,
        ),
        ("goto done\ndone:\n\treturn", 1),
    ]);
}

/// Correct by omission: `fallthrough_statement` is in no kind list. It continues into the next
/// case, which the switch increment already paid for.
#[test]
fn fallthrough_scores_zero() {
    assert_scores(&[("switch a { case 1: fallthrough; case 2: f() }", 1)]);
}

/// Correct by omission: `defer_statement` and `go_statement` are in no kind list. The literal
/// they launch still raises nesting, exactly as any closure does.
#[test]
fn defer_and_go_are_free_but_their_literals_nest() {
    assert_scores(&[
        ("defer f()", 0),
        ("go f()", 0),
        ("defer func() { if r := recover(); r != nil { f() } }()", 2),
        ("go func() { f() }()", 0),
    ]);
}

/// A literal scores +0 but raises the nesting level, and the total is attributed to the
/// enclosing unit.
#[test]
fn closures_raise_nesting_without_scoring() {
    assert_scores(&[
        ("r := func() { if a { f() } }; r()", 2),
        ("if x { r := func() { if a { f() } }; r() }", 4),
    ]);
}

#[test]
fn callback_nesting_compounds_into_the_enclosing_unit() {
    assert_scores(&[(
        "if a { for _, x := range xs { each(x, func() { if b { f() } }) } }",
        7,
    )]);
}

#[test]
fn direct_recursion_costs_one() {
    assert_scores(&[("target()", 1), ("other.target()", 0), ("unrelated()", 0)]);
}

fn score_of(source: &str, qualified: &str) -> u32 {
    findings(source)
        .into_iter()
        .find(|finding| finding.qualified_name() == qualified)
        .unwrap_or_else(|| panic!("no unit named {qualified} in:\n{source}"))
        .score
}

#[test]
fn a_method_recurses_through_its_receiver() {
    let source = "package p\n\
        func (s *Stack) Push(v int) { s.Push(v) }\n\
        func (s Stack) Len() int { return Stack.Len(s) }\n\
        func (s *Stack) Walk() { each(func() { s.Walk() }) }\n";
    assert_eq!(score_of(source, "Stack::Push"), 1);
    assert_eq!(score_of(source, "Stack::Len"), 1);
    assert_eq!(score_of(source, "Stack::Walk"), 1);
}

/// A method cannot be called without its receiver, so a bare call of its own name is a free
/// function, and a call through another value is another object's method.
#[test]
fn a_method_does_not_recurse_through_anything_else() {
    let source = "package p\n\
        func (s *Stack) Pop() { Pop() }\n\
        func (s *Stack) Peek() { s.inner.Peek() }\n\
        func (s *Stack) Drop(o *Stack) { o.Drop(s) }\n";
    assert_eq!(score_of(source, "Stack::Pop"), 0);
    assert_eq!(score_of(source, "Stack::Peek"), 0);
    assert_eq!(score_of(source, "Stack::Drop"), 0);
}

#[test]
fn a_wrapper_around_another_packages_namesake_is_not_recursion() {
    let source = "package p\nfunc Split(s string) []string { return strings.Split(s, \",\") }\n";
    assert_eq!(score_of(source, "Split"), 0);
}

#[test]
fn a_linear_function_scores_zero() {
    assert_scores(&[("a := 1; b := 2; _ = a + b", 0)]);
}
