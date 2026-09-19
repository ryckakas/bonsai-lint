//! JSX-specific behaviour. The reference JavaScript analyser exempts short-circuit rendering;
//! we count it, because it is a branch and a ternary in the same position unambiguously scores.

mod common;

use common::{findings_in, score};

#[test]
fn jsx_short_circuit_counts_as_a_logical_run() {
    assert_eq!(score("return <div>{a && <A />}</div>;"), 1);
    assert_eq!(score("return <div>{a && b && <A />}</div>;"), 1);
    assert_eq!(score("return <div>{a && <A />}{b && <B />}</div>;"), 2);
}

/// Counting `&&` but not `?:` would create an incentive to reach for whichever the linter
/// ignores.
#[test]
fn jsx_ternary_and_short_circuit_agree() {
    assert_eq!(score("return <div>{a ? <A /> : null}</div>;"), 1);
    assert_eq!(score("return <div>{a && <A />}</div>;"), 1);
}

/// A boolean run is a fundamental increment: +1, with no nesting term. Nesting compounds only
/// for structural increments, so a JSX guard costs the same wherever it sits. A component pays
/// one point per conditional render, flat.
#[test]
fn jsx_guards_cost_one_each_regardless_of_nesting() {
    let flat = "return <ul>{a && <A />}</ul>;";
    let nested = "return <ul>{xs.map(x => <li>{x.items.map(i => <b>{a && <A />}</b>)}</li>)}</ul>;";
    assert_eq!(score(flat), 1);
    assert_eq!(score(nested), 1);
}

/// Structural increments inside the same callbacks do compound, which is what keeps deeply
/// nested rendering logic expensive even though the guards themselves are flat.
#[test]
fn structural_increments_inside_jsx_callbacks_compound() {
    assert_eq!(
        score("return <ul>{xs.map(x => <li>{x.ok ? <A /> : <B />}</li>)}</ul>;"),
        2
    );
    assert_eq!(
        score(
            "return <ul>{xs.map(x => <li>{x.items.map(i => (i.ok ? <A /> : <B />))}</li>)}</ul>;"
        ),
        3
    );
}

/// `.ts` must use the TypeScript grammar, where angle brackets are a type assertion rather than
/// a JSX element.
#[test]
fn the_typescript_dialect_parses_angle_bracket_assertions() {
    let source = "function target() { const x = <string>value; return x; }";
    let findings = findings_in(&bonsai_lang_ts::TYPESCRIPT, source);
    assert!(findings.iter().any(|finding| finding.name == "target"));
}
