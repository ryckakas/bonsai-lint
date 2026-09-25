//! Go allows only declarations at package scope, so file-level logic is whatever a `var`
//! initialiser computes when the package loads.

mod common;

use common::findings;

fn toplevel_score(source: &str) -> Option<u32> {
    findings(source)
        .into_iter()
        .find(|finding| finding.name == bonsai_core::TOPLEVEL_UNIT)
        .map(|finding| finding.score)
}

#[test]
fn package_level_initialisers_are_scored() {
    let source = "package p\n\nvar ok = a && b\nvar mixed = a && b || c\n";
    assert_eq!(toplevel_score(source), Some(3));
}

#[test]
fn declared_units_are_not_counted_twice() {
    let source = "package p\n\nvar ok = a && b\nfunc foo() { if b { f() } }\n";
    assert_eq!(toplevel_score(source), Some(1));
}

#[test]
fn a_package_level_literal_is_a_unit_not_file_logic() {
    let source = "package p\n\nvar handler = func() { if a { f() } }\n";
    assert_eq!(toplevel_score(source), None);

    let handler = findings(source)
        .into_iter()
        .find(|finding| finding.name == "handler")
        .expect("handler is a unit");
    assert_eq!(handler.score, 1);
}

#[test]
fn a_file_without_top_level_logic_reports_nothing() {
    let source = "package p\n\nimport \"fmt\"\n\nfunc (s *S) Print() { fmt.Println(s) }\n";
    assert_eq!(toplevel_score(source), None);
}

/// Deriving the line from the root node instead would report 4 here: tree-sitter starts the root
/// at the first token, not at byte 0.
#[test]
fn leading_blank_lines_do_not_move_the_toplevel_line() {
    let findings = findings("\n\n\npackage p\n\nvar ok = a && b\n");
    let toplevel = findings
        .iter()
        .find(|finding| finding.name == bonsai_core::TOPLEVEL_UNIT)
        .expect("top-level code scores");
    assert_eq!(toplevel.line, 1);
}
