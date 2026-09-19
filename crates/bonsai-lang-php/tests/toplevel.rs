//! Code outside any function is where procedural scripts and module-level initialisation hide,
//! so it is scored as a synthetic unit rather than left invisible.

mod common;

use common::findings;

fn toplevel_score(source: &str) -> Option<u32> {
    findings(source)
        .into_iter()
        .find(|finding| finding.name == bonsai_core::TOPLEVEL_UNIT)
        .map(|finding| finding.score)
}

#[test]
fn statements_outside_a_function_are_scored() {
    let source = "<?php\nif ($a) { echo 1; }\nforeach ($b as $c) { if ($d) { echo 2; } }\n";
    assert_eq!(toplevel_score(source), Some(4));
}

/// The top-level pass must stop at anything `collect` already reports, or a declared function
/// would be counted once as its own unit and again as part of the file.
#[test]
fn declared_units_are_not_counted_twice() {
    let source = "<?php\nif ($a) { echo 1; }\nfunction foo() { if ($b) { echo 1; } }\n";

    assert_eq!(toplevel_score(source), Some(1));

    let foo = findings(source)
        .into_iter()
        .find(|f| f.name == "foo")
        .expect("foo is a unit");
    assert_eq!(foo.score, 1);
}

#[test]
fn class_methods_are_not_counted_into_the_file() {
    let source = "<?php\nclass A { public function m() { if ($b) { echo 1; } } }\n";
    assert_eq!(toplevel_score(source), None);
}

/// Most files legitimately have no top-level logic, so emitting a zero for every one of them
/// would drown the report.
#[test]
fn a_file_without_top_level_logic_reports_nothing() {
    assert_eq!(
        toplevel_score("<?php\nfunction foo() { return 1; }\n"),
        None
    );
}
