//! Code outside any function is where module-level bootstrap and configuration objects hide, so
//! it is scored as a synthetic unit rather than left invisible.

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
    let source = "if (a) { f(); }\nfor (const x of xs) { if (x) { f(); } }\n";
    assert_eq!(toplevel_score(source), Some(4));
}

#[test]
fn declared_units_are_not_counted_twice() {
    let source = "if (a) { f(); }\nfunction foo() { if (b) { f(); } }\n";
    assert_eq!(toplevel_score(source), Some(1));
}

/// An object literal is a container for the units it holds, but its own values run when the
/// module loads and belong to the file.
#[test]
fn object_literal_logic_at_file_scope_is_scored() {
    let source = "export default { mode: flag ? 'a' : 'b', list: xs || [] };\n";
    assert_eq!(toplevel_score(source), Some(2));
}

#[test]
fn methods_inside_a_file_scope_object_are_units_not_file_logic() {
    let source = "export default { mode: flag ? 'a' : 'b', onClick() { if (a) { f(); } } };\n";
    assert_eq!(toplevel_score(source), Some(1));

    let on_click = findings(source)
        .into_iter()
        .find(|finding| finding.name == "onClick")
        .expect("onClick is a unit");
    assert_eq!(on_click.score, 1);
}

#[test]
fn class_field_initialisers_belong_to_the_file() {
    let source = "class A {\n  mode = flag ? 'a' : 'b';\n  run() { if (a) { f(); } }\n}\n";
    assert_eq!(toplevel_score(source), Some(1));
}

#[test]
fn a_file_without_top_level_logic_reports_nothing() {
    assert_eq!(toplevel_score("function foo() { return 1; }\n"), None);
}

/// Deriving the line from the root node instead would report 4 here: tree-sitter starts the root
/// at the first token, not at byte 0.
#[test]
fn leading_blank_lines_do_not_move_the_toplevel_line() {
    let findings = findings("\n\n\nif (a) { b(); }\n");
    let toplevel = findings
        .iter()
        .find(|finding| finding.name == bonsai_core::TOPLEVEL_UNIT)
        .expect("top-level code scores");
    assert_eq!(toplevel.line, 1);
}
