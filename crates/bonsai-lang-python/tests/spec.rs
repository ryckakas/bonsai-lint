//! Conformance against the cognitive complexity scoring rules.

mod common;

use common::{assert_scores, score_of};

#[test]
fn operator_sequences_cost_per_run_not_per_operator() {
    assert_scores(&[
        ("if a and b and c:\n    f()", 2),
        ("if a and b or c:\n    f()", 3),
        ("if a and b and c or d or e and g:\n    f()", 4),
        ("x = a and b", 1),
    ]);
}

/// A run that returns to an operator after switching away costs again. Other implementations
/// count each operator once per group, which is a documented divergence.
#[test]
fn returning_to_an_operator_starts_a_new_run() {
    assert_scores(&[
        ("x = a and b or c and d", 3),
        ("x = (a and b) or (c and d)", 3),
    ]);
}

#[test]
fn parentheses_do_not_break_an_operator_run() {
    assert_scores(&[
        ("if a and (b and c):\n    f()", 2),
        ("if a and (b or c):\n    f()", 3),
        ("if a and (b or c) or d:\n    f()", 3),
        ("if a and (b or c) and d:\n    f()", 4),
    ]);
}

#[test]
fn negation_starts_a_new_sequence() {
    assert_scores(&[("if a and not (b and c):\n    f()", 3)]);
}

#[test]
fn bitwise_operators_score_zero() {
    assert_scores(&[("x = a & b | c", 0), ("x = a ^ b", 0)]);
}

#[test]
fn nesting_compounds() {
    assert_scores(&[
        ("if a:\n    f()", 1),
        ("if a:\n    if b:\n        f()", 3),
        ("if a:\n    if b:\n        if c:\n            f()", 6),
        (
            "for x in xs:\n    if x > 0:\n        while a:\n            f()",
            6,
        ),
    ]);
}

#[test]
fn every_loop_form_costs_one_plus_nesting() {
    assert_scores(&[
        ("for x in xs:\n    f()", 1),
        ("while a:\n    f()", 1),
        ("if a:\n    while b:\n        f()", 3),
    ]);
}

#[test]
fn async_forms_score_like_their_sync_ones() {
    let source = "async def target():\n    async for x in xs:\n        async with a:\n            if await b(x):\n                f()\n";
    assert_eq!(score_of(source, "target"), 3);
}

#[test]
fn loop_headers_do_not_nest() {
    assert_scores(&[
        ("for i in range(n if a else m):\n    f()", 2),
        ("while any(map(lambda x: 1 if x else 0, xs)):\n    f()", 3),
        ("match a if b else c:\n    case 1:\n        f()", 2),
    ]);
}

#[test]
fn else_branches_take_a_flat_increment() {
    assert_scores(&[
        ("if a:\n    f()\nelif b:\n    g()\nelse:\n    h()", 3),
        ("if a:\n    f()\nelif b:\n    g()", 2),
        ("if a:\n    f()\nelse:\n    g()", 2),
        (
            "for x in xs:\n    if a:\n        f()\n    elif b:\n        g()\n    else:\n        h()",
            5,
        ),
        ("if a:\n    f()\nelif b:\n    if c:\n        g()", 4),
    ]);
}

/// Every body of a chain sits one level below its `if`, however many links precede it. Other
/// implementations nest each further link deeper, which is a documented divergence.
#[test]
fn a_long_chain_does_not_nest_its_later_links() {
    assert_scores(&[
        (
            "if a:\n    f()\nelif b:\n    g()\nelse:\n    x = 1 if c else 2",
            5,
        ),
        (
            "if a:\n    f()\nelif b:\n    g()\nelif c:\n    h()\nelse:\n    x = 1 if d else 2",
            6,
        ),
    ]);
}

/// Python spells a chain `elif`, so an `if` alone under `else:` is nested code.
#[test]
fn an_if_alone_inside_an_else_still_nests() {
    assert_scores(&[("if a:\n    f()\nelse:\n    if b:\n        g()", 4)]);
}

/// A loop's `else:` runs only when the loop didn't break, and a try's only when nothing was
/// raised: branches like any other else. Each body sits where its construct's own body does.
#[test]
fn loop_and_try_else_take_a_flat_increment() {
    assert_scores(&[
        ("for x in xs:\n    if x:\n        break\nelse:\n    f()", 4),
        ("while a:\n    f()\nelse:\n    g()", 2),
        ("try:\n    f()\nexcept E:\n    g()\nelse:\n    h()", 2),
        ("for x in xs:\n    f()\nelse:\n    if a:\n        g()", 4),
        (
            "try:\n    f()\nexcept E:\n    g()\nelse:\n    if a:\n        h()",
            3,
        ),
    ]);
}

#[test]
fn match_costs_one_regardless_of_case_count() {
    assert_scores(&[(
        "match x:\n    case 1:\n        f()\n    case 2:\n        g()\n    case _:\n        h()",
        1,
    )]);
}

#[test]
fn match_cases_nest() {
    assert_scores(&[("match x:\n    case 1:\n        if a:\n            f()", 3)]);
}

#[test]
fn a_guard_costs_only_its_operators() {
    assert_scores(&[
        ("match x:\n    case Foo() if a:\n        f()", 1),
        ("match x:\n    case Foo() if a and b:\n        f()", 2),
    ]);
}

#[test]
fn try_is_free_and_each_except_costs_one() {
    assert_scores(&[
        ("try:\n    f()\nexcept E:\n    g()", 1),
        ("try:\n    f()\nexcept E:\n    if a:\n        g()", 3),
        ("try:\n    f()\nfinally:\n    g()", 0),
        ("try:\n    f()\nexcept (A, B) as e:\n    g()", 1),
        ("try:\n    f()\nexcept A:\n    g()\nexcept B:\n    h()", 2),
        ("try:\n    f()\nexcept* E:\n    g()", 1),
        ("try:\n    f()\nexcept:\n    g()", 1),
    ]);
}

#[test]
fn with_is_free_and_does_not_nest() {
    assert_scores(&[
        ("with open(p) as handle:\n    f(handle)", 0),
        ("with a:\n    if b:\n        f()", 1),
    ]);
}

#[test]
fn ternaries_cost_one_plus_nesting() {
    assert_scores(&[
        ("y = 1 if a else 2", 1),
        ("y = 1 if a else 2 if b else 3", 3),
        ("y = (1 if b else 2) if a else 3", 3),
    ]);
}

/// The grammar leaves the condition unfielded between the two branches, so the language names
/// it header, where TypeScript's and Java's conditions already are.
#[test]
fn a_ternary_condition_is_header() {
    assert_scores(&[("y = 1 if (2 if b else 3) else 4", 2)]);
}

/// Python has no labels, so a jump never costs more than the loop around it.
#[test]
fn break_and_continue_are_free() {
    assert_scores(&[
        ("for x in xs:\n    break", 1),
        ("for x in xs:\n    continue", 1),
        ("for x in xs:\n    for y in x:\n        break", 3),
    ]);
}

#[test]
fn statements_that_do_not_branch_score_zero() {
    assert_scores(&[
        ("assert a is not None", 0),
        ("raise ValueError()", 0),
        ("del cache[key]", 0),
        ("global counter", 0),
        ("yield x", 0),
        ("return", 0),
        ("pass", 0),
        ("if (n := len(a)) > 1:\n    f()", 1),
    ]);
}

/// A comprehension is what JavaScript writes as a `.filter().map()` chain, so it nests like a
/// closure and costs nothing itself. Other implementations score its clauses, or ignore it.
#[test]
fn comprehensions_nest_without_scoring() {
    assert_scores(&[
        ("ys = [1 if x.a else 2 for x in xs]", 2),
        ("ys = [x for x in xs if x]", 0),
        ("ys = [x for r in m for x in r if a and b]", 1),
        ("ys = [[1 if c else 0 for c in row] for row in m]", 3),
        ("ys = {x for x in xs}", 0),
        ("ys = {k: v for k, v in d.items() if v}", 0),
        ("ys = {k: (1 if v else 0) for k, v in d}", 2),
        ("ys = any(x > 0 for x in xs)", 0),
    ]);
}

#[test]
fn closures_raise_nesting_without_scoring() {
    assert_scores(&[
        ("g = lambda x: 1 if x else 2", 2),
        ("g = lambda: h()", 0),
        ("def inner():\n    if a:\n        f()", 2),
    ]);
}

#[test]
fn a_local_class_method_nests_like_a_closure() {
    assert_scores(&[(
        "class Local:\n    def m(self):\n        if a:\n            f()",
        2,
    )]);
}

#[test]
fn callback_nesting_compounds_into_the_enclosing_unit() {
    assert_scores(&[(
        "if a:\n    for x in xs:\n        def cb(y):\n            if b:\n                f()",
        7,
    )]);
}

/// Other implementations exempt a function holding only a nested def and its return, which is
/// a documented divergence: the same shape nests in every other language.
#[test]
fn a_decorator_factory_nests_its_wrapper() {
    let source = "def retry(f):\n    @wraps(f)\n    def wrapper(*a):\n        if flaky():\n            f()\n    return wrapper\n";
    assert_eq!(score_of(source, "retry"), 2);
}

#[test]
fn direct_recursion_costs_one() {
    assert_scores(&[("target()", 1), ("other.target()", 0)]);
}

#[test]
fn a_method_recurses_through_self_cls_or_its_class() {
    for call in ["self.target()", "cls.target()", "C.target()"] {
        let source = format!("class C:\n    def target(self):\n        {call}\n");
        assert_eq!(score_of(&source, "C::target"), 1, "{call}");
    }
}

/// A nested class's own name is not in scope inside its methods, so only its full path reaches
/// the method; a bare `Inner` names a global, as a bare call does.
#[test]
fn a_nested_class_method_recurses_only_through_its_full_path() {
    let full = "class Outer:\n    class Inner:\n        def target(self):\n            Outer.Inner.target(self)\n";
    assert_eq!(score_of(full, "Outer::Inner::target"), 1);

    let short = "class Outer:\n    class Inner:\n        def target(self):\n            Inner.target(self)\n";
    assert_eq!(score_of(short, "Outer::Inner::target"), 0);
}

/// Inside a method a bare name resolves to a global, never to the method itself.
#[test]
fn a_bare_call_inside_a_method_is_not_recursion() {
    let source = "class C:\n    def target(self):\n        target()\n";
    assert_eq!(score_of(source, "C::target"), 0);
}

#[test]
fn a_call_to_the_parent_implementation_is_not_recursion() {
    let source = "class C(Base):\n    def target(self):\n        super().target()\n";
    assert_eq!(score_of(source, "C::target"), 0);
}

#[test]
fn recursion_inside_a_lambda_still_reaches_the_function() {
    assert_scores(&[("g = lambda: target()", 1)]);
}

#[test]
fn a_linear_function_scores_zero() {
    assert_scores(&[("x = 1\ny = x + 2\nreturn y", 0)]);
}
