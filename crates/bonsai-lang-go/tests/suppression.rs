//! A marker without a reason is refused rather than obeyed, so silencing a finding stays a
//! documented decision. Every binding shape the namer knows must also be suppressible.

mod common;

use bonsai_core::Suppression;
use common::findings;

fn suppression_of(source: &str, name: &str) -> Suppression {
    findings(source)
        .into_iter()
        .find(|finding| finding.name == name)
        .unwrap_or_else(|| panic!("no finding named {name} in:\n{source}"))
        .suppression
}

fn reasoned(reason: &str) -> Suppression {
    Suppression::Reasoned(reason.to_string())
}

#[test]
fn a_reasoned_marker_above_the_declaration_is_honoured() {
    let source =
        "package p\n\n// bonsai-lint-ignore: parser state machine\nfunc target() { if a { f() } }\n";
    assert_eq!(
        suppression_of(source, "target"),
        reasoned("parser state machine")
    );
}

/// Go documentation is a run of line comments, so the marker may sit anywhere inside it.
#[test]
fn a_reasoned_marker_inside_a_doc_comment_is_honoured() {
    let source = "package p\n\n// Target dispatches.\n//\n// bonsai-lint-ignore: generated dispatch table\n// See the spec.\nfunc target() { if a { f() } }\n";
    assert_eq!(
        suppression_of(source, "target"),
        reasoned("generated dispatch table")
    );
}

#[test]
fn a_trailing_marker_on_the_signature_line_is_honoured() {
    let source = "package p\n\nfunc target() { // bonsai-lint-ignore: legacy\n\tif a { f() }\n}\n";
    assert_eq!(suppression_of(source, "target"), reasoned("legacy"));
}

#[test]
fn a_bare_marker_is_refused() {
    let source = "package p\n\n// bonsai-lint-ignore\nfunc target() { if a { f() } }\n";
    assert_eq!(suppression_of(source, "target"), Suppression::MissingReason);
}

#[test]
fn a_marker_above_a_method_is_honoured() {
    let source =
        "package p\n\n// bonsai-lint-ignore: hot path\nfunc (s *Stack) Push(v int) { if a { f() } }\n";
    assert_eq!(suppression_of(source, "Push"), reasoned("hot path"));
}

#[test]
fn a_marker_above_a_bound_literal_is_honoured() {
    let source =
        "package p\n\n// bonsai-lint-ignore: hand-tuned\nvar handler = func() { if a { f() } }\n";
    assert_eq!(suppression_of(source, "handler"), reasoned("hand-tuned"));
}

/// The namer hands a lone callback the binding of the call wrapping it, so the marker above that
/// binding must cover it too.
#[test]
fn a_marker_above_a_wrapped_literal_is_honoured() {
    let source = "package p\n\n// bonsai-lint-ignore: route table\nvar handler = wrap(func() { if a { f() } })\n";
    assert_eq!(suppression_of(source, "handler"), reasoned("route table"));
}

#[test]
fn a_marker_inside_a_var_group_covers_only_its_own_spec() {
    let source = "package p\n\nvar (\n\t// bonsai-lint-ignore: vendored\n\thandler = func() { if a { f() } }\n\tother = func() { if b { f() } }\n)\n";
    assert_eq!(suppression_of(source, "handler"), reasoned("vendored"));
    assert_eq!(suppression_of(source, "other"), Suppression::None);
}

#[test]
fn a_marker_above_a_map_entry_is_honoured() {
    let source = "package p\n\nvar routes = map[string]func(){\n\t\"index\": func() {},\n\t// bonsai-lint-ignore: legacy route\n\t\"list\": func() { if a { f() } },\n}\n";
    assert_eq!(suppression_of(source, "list"), reasoned("legacy route"));
    assert_eq!(suppression_of(source, "index"), Suppression::None);
}

#[test]
fn a_marker_cannot_leak_into_the_next_declaration() {
    let source = "package p\n\n// bonsai-lint-ignore: only the first\nfunc first() { if a { f() } }\nfunc second() { if b { f() } }\n";
    assert_eq!(suppression_of(source, "second"), Suppression::None);
}

#[test]
fn a_trailing_marker_on_a_closing_brace_line_suppresses_nothing() {
    let source = "package p\n\nfunc first() {\n\tif a { f() }\n} // bonsai-lint-ignore: nobody's\nfunc second() { if b { f() } }\n";
    assert_eq!(suppression_of(source, "first"), Suppression::None);
    assert_eq!(suppression_of(source, "second"), Suppression::None);
}

/// File-level code is reported on line 1, so its marker sits in the comment block at the top of
/// the file, above or just after the package clause.
#[test]
fn a_marker_above_the_package_clause_suppresses_the_file() {
    let source = "// bonsai-lint-ignore: flag table\npackage p\n\nvar ok = a && b\n";
    assert_eq!(suppression_of(source, "<toplevel>"), reasoned("flag table"));
}

#[test]
fn a_marker_behind_the_package_clause_suppresses_the_file() {
    let source = "package p\n\n// bonsai-lint-ignore: flag table\n\nvar ok = a && b\n";
    assert_eq!(suppression_of(source, "<toplevel>"), reasoned("flag table"));
}

#[test]
fn a_marker_above_the_first_function_belongs_to_it_not_the_file() {
    let source = "package p\n\n// bonsai-lint-ignore: reason\nfunc target() { if a { f() } }\n\nvar ok = a && b\n";
    assert_eq!(suppression_of(source, "target"), reasoned("reason"));
    assert_eq!(suppression_of(source, "<toplevel>"), Suppression::None);
}

#[test]
fn a_bare_marker_at_the_top_of_the_file_is_refused() {
    let source = "// bonsai-lint-ignore\npackage p\n\nvar ok = a && b\n";
    assert_eq!(
        suppression_of(source, "<toplevel>"),
        Suppression::MissingReason
    );
}
