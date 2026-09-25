//! Java has no code outside a class, but field initialisers and instance initializer blocks run
//! when a class is set up, and belong to the file rather than to any method.

mod common;

use common::findings;

fn toplevel_score(source: &str) -> Option<u32> {
    findings(source)
        .into_iter()
        .find(|finding| finding.name == bonsai_core::TOPLEVEL_UNIT)
        .map(|finding| finding.score)
}

#[test]
fn field_initialisers_are_scored() {
    let source = "class C { static final boolean OK = a && b; }\n";
    assert_eq!(toplevel_score(source), Some(1));
}

#[test]
fn instance_initializer_logic_is_scored() {
    let source = "class C { { if (a) { f(); } } }\n";
    assert_eq!(toplevel_score(source), Some(1));
}

#[test]
fn declared_units_are_not_counted_twice() {
    let source = "class C {\n  static final boolean OK = a && b;\n  static { if (a) { f(); } }\n  Runnable r = () -> { if (b) { f(); } };\n  void m() { if (c) { f(); } }\n}\n";
    assert_eq!(toplevel_score(source), Some(1));
}

#[test]
fn a_file_without_class_level_logic_reports_nothing() {
    let source = "package p;\n\nimport java.util.List;\n\nclass C {\n  private int x = 1;\n  void m() { if (a) { f(); } }\n}\n";
    assert_eq!(toplevel_score(source), None);
}

#[test]
fn a_package_info_file_reports_nothing() {
    let source = "/** Docs. */\n@NonNullApi\npackage com.example;\n";
    assert!(findings(source).is_empty());
}

#[test]
fn leading_blank_lines_do_not_move_the_toplevel_line() {
    let source = "\n\nclass C { static final boolean OK = a && b; }\n";
    let line = findings(source)
        .into_iter()
        .find(|finding| finding.name == bonsai_core::TOPLEVEL_UNIT)
        .map(|finding| finding.line);
    assert_eq!(line, Some(1));
}
