//! Conformance against the cognitive complexity scoring rules.
//!
//! Each case states the increments it expects so a failure says which rule broke, not just
//! which number moved.

mod common;

use common::assert_scores;

#[test]
fn operator_sequences_cost_per_run_not_per_operator() {
    assert_scores(&[
        ("if ($a && $b && $c) { echo 1; }", 2),
        ("if ($a && $b || $c) { echo 1; }", 3),
        ("if ($a && $b && $c || $d || $e && $f) { echo 1; }", 4),
        ("if ($a and $b and $c) { echo 1; }", 2),
    ]);
}

/// Parentheses are skipped when flattening a sequence, so grouping alone does not start a new
/// run. Only a change of operator does.
#[test]
fn parentheses_do_not_break_an_operator_run() {
    assert_scores(&[
        // if +1, one `&&` run across the parentheses +1
        ("if ($a && ($b && $c)) { echo 1; }", 2),
        // if +1, `&&` run then `||` run +2
        ("if ($a && ($b || $c)) { echo 1; }", 3),
        // if +1, `&&` then `||` — the trailing `|| $d` continues the same `||` run
        ("if ($a && ($b || $c) || $d) { echo 1; }", 3),
    ]);
}

/// A negation is not a logical expression, so it ends the sequence and its contents are scored
/// as a fresh one, which is what `a && !(b && c)` costing 3 reflects.
#[test]
fn negation_starts_a_new_sequence() {
    assert_scores(&[("if ($a && !($b && $c)) { echo 1; }", 3)]);
}

/// Correct by omission: `??` is absent from the operator normaliser. Without this test, adding
/// it would change every score in the wild and nothing would go red.
#[test]
fn null_coalescing_scores_zero() {
    assert_scores(&[
        ("if ($a ?? $b) { echo 1; }", 1),
        ("$x = $a ?? $b ?? $c; return $x;", 0),
    ]);
}

#[test]
fn nesting_compounds() {
    assert_scores(&[
        ("if ($a) { echo 1; }", 1),
        ("if ($a) { if ($b) { echo 1; } }", 3),
        ("if ($a) { if ($b) { if ($c) { echo 1; } } }", 6),
        ("foreach ($xs as $x) { if ($x) { echo 1; } }", 3),
    ]);
}

#[test]
fn else_branches_take_a_flat_increment() {
    assert_scores(&[
        (
            "if ($a) { echo 1; } elseif ($b) { echo 2; } else { echo 3; }",
            3,
        ),
        ("if ($a) { echo 1; } else if ($b) { echo 2; }", 2),
        ("if ($a) { echo 1; } elseif ($b) { echo 2; }", 2),
    ]);
}

#[test]
fn switch_and_match_cost_one_regardless_of_arm_count() {
    assert_scores(&[
        (
            "switch ($a) { case 1: echo 1; break; case 2: echo 2; break; default: echo 3; }",
            1,
        ),
        ("$r = match ($a) { 1 => 'a', 2 => 'b', default => 'c' };", 1),
    ]);
}

#[test]
fn try_is_free_and_each_catch_costs_one() {
    assert_scores(&[
        ("try { foo(); } catch (Exception $e) { echo 1; }", 1),
        (
            "try { foo(); } catch (A $e) { echo 1; } catch (B $e) { echo 2; }",
            2,
        ),
        (
            "try { foo(); } catch (Exception $e) { if ($a) { echo 1; } }",
            3,
        ),
    ]);
}

#[test]
fn multi_level_jumps_cost_one_and_plain_jumps_are_free() {
    assert_scores(&[
        ("foreach ($a as $x) { break; }", 1),
        ("foreach ($a as $x) { foreach ($x as $y) { break 2; } }", 4),
        ("goto end;", 1),
    ]);
}

/// A lambda scores +0 itself but raises the nesting level, and the total is attributed to the
/// enclosing unit.
#[test]
fn closures_raise_nesting_without_scoring() {
    assert_scores(&[
        ("$f = function () { if ($a) { echo 1; } };", 2),
        ("$f = fn () => $a ? 1 : 2;", 2),
        ("if ($a) { $f = function () { if ($b) { echo 1; } }; }", 4),
    ]);
}

#[test]
fn direct_recursion_costs_one() {
    assert_scores(&[
        ("return target();", 1),
        ("return $this->target();", 1),
        ("return $other->target();", 0),
        ("return unrelated();", 0),
    ]);
}

#[test]
fn a_linear_function_scores_zero() {
    assert_scores(&[("$a = 1; $b = 2; return $a + $b;", 0)]);
}

/// A loop header is read before its body, so a ternary in the `foreach` subject or in a `for`
/// initialiser sits at the loop's own nesting level, not one deeper.
#[test]
fn loop_headers_do_not_nest() {
    assert_scores(&[
        ("foreach ($a ? $xs : [] as $x) { echo $x; }", 2),
        (
            "for ($i = $a ? 0 : 1; $i < 3; $i = $b ? $i + 1 : $i + 2) { echo $i; }",
            3,
        ),
    ]);
}
