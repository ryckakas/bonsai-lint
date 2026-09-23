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

fn line_of(source: &str, name: &str) -> usize {
    findings(source)
        .into_iter()
        .find(|finding| finding.name == name)
        .unwrap_or_else(|| panic!("no finding named {name} in:\n{source}"))
        .line
}

#[test]
fn a_reasoned_marker_above_the_declaration_is_honoured() {
    let source =
        "// bonsai-lint-ignore: parser state machine\nfunction target() { if (a) { f(); } }\n";
    assert_eq!(
        suppression_of(source, "target"),
        Suppression::Reasoned("parser state machine".to_string())
    );
}

#[test]
fn a_reasoned_marker_in_a_docblock_is_honoured() {
    let source = "/**\n * bonsai-lint-ignore: generated dispatch table\n */\nfunction target() { if (a) { f(); } }\n";
    assert_eq!(
        suppression_of(source, "target"),
        Suppression::Reasoned("generated dispatch table".to_string())
    );
}

#[test]
fn a_trailing_marker_on_the_signature_line_is_honoured() {
    let source = "function target() { // bonsai-lint-ignore: legacy\n    if (a) { f(); }\n}\n";
    assert_eq!(
        suppression_of(source, "target"),
        Suppression::Reasoned("legacy".to_string())
    );
}

#[test]
fn a_bare_marker_is_refused() {
    let source = "// bonsai-lint-ignore\nfunction target() { if (a) { f(); } }\n";
    assert_eq!(suppression_of(source, "target"), Suppression::MissingReason);
}

#[test]
fn a_marker_above_a_bound_arrow_is_honoured() {
    let source =
        "// bonsai-lint-ignore: hand-tuned\nexport const handler = () => { if (a) { f(); } };\n";
    assert_eq!(
        suppression_of(source, "handler"),
        Suppression::Reasoned("hand-tuned".to_string())
    );
}

/// The namer hands a lone callback the binding of the call wrapping it, so the marker above that
/// binding must cover it too.
#[test]
fn a_marker_above_a_wrapped_arrow_is_honoured() {
    let source = "// bonsai-lint-ignore: store factory\nexport const useCart = defineStore('cart', () => { if (a) { f(); } });\n";
    assert_eq!(
        suppression_of(source, "useCart"),
        Suppression::Reasoned("store factory".to_string())
    );
}

#[test]
fn a_marker_above_a_class_field_arrow_is_honoured() {
    let source = "class F {\n  // bonsai-lint-ignore: framework contract\n  field = () => { if (a) { f(); } };\n}\n";
    assert_eq!(
        suppression_of(source, "field"),
        Suppression::Reasoned("framework contract".to_string())
    );
}

#[test]
fn a_marker_above_an_object_pair_is_honoured() {
    let source = "const api = {\n  first: 1,\n  // bonsai-lint-ignore: vendored\n  onClick: function () { if (a) { f(); } },\n};\n";
    assert_eq!(
        suppression_of(source, "onClick"),
        Suppression::Reasoned("vendored".to_string())
    );
}

#[test]
fn a_marker_above_a_decorated_method_is_honoured() {
    let source = "class A {\n  // bonsai-lint-ignore: framework contract\n  @Get('/x')\n  target() { if (a) { f(); } }\n}\n";
    assert_eq!(
        suppression_of(source, "target"),
        Suppression::Reasoned("framework contract".to_string())
    );
}

#[test]
fn a_marker_cannot_leak_into_the_next_declaration() {
    let source = "// bonsai-lint-ignore: only the first\nfunction first() { if (a) { f(); } }\nfunction second() { if (b) { f(); } }\n";
    assert_eq!(suppression_of(source, "second"), Suppression::None);
}

/// On one line, the closing brace shares the signature line, so the marker is the declaration's
/// own; it must never reach the declaration below.
#[test]
fn a_trailing_marker_after_a_one_line_declaration_belongs_to_it() {
    let source = "function first() { if (a) { f(); } } // bonsai-lint-ignore: mine\nfunction second() { if (b) { f(); } }\n";
    assert_eq!(
        suppression_of(source, "first"),
        Suppression::Reasoned("mine".to_string())
    );
    assert_eq!(suppression_of(source, "second"), Suppression::None);
}

#[test]
fn a_trailing_marker_on_a_closing_brace_line_suppresses_nothing() {
    let source = "function first() {\n  if (a) { f(); }\n} // bonsai-lint-ignore: nobody's\nfunction second() { if (b) { f(); } }\n";
    assert_eq!(suppression_of(source, "first"), Suppression::None);
    assert_eq!(suppression_of(source, "second"), Suppression::None);
}

#[test]
fn a_decorator_does_not_move_the_reported_line() {
    let source = "class A {\n  @Get('/x')\n  @Auth()\n  handler() { if (a) { f(); } }\n}\n";
    assert_eq!(line_of(source, "handler"), 4);
}

/// File-level code is reported on line 1, so its marker sits in the comment block at the top of
/// the file, behind a shebang if there is one.
#[test]
fn a_marker_on_the_first_line_suppresses_the_file() {
    let source = "// bonsai-lint-ignore: module bootstrap\nif (a) { if (b) { run(); } }\n";
    assert_eq!(
        suppression_of(source, "<toplevel>"),
        Suppression::Reasoned("module bootstrap".to_string())
    );
}

#[test]
fn a_marker_behind_a_shebang_suppresses_the_file() {
    let source = "#!/usr/bin/env node\n// bonsai-lint-ignore: cli entry point\nif (a) { run(); }\n";
    assert_eq!(
        suppression_of(source, "<toplevel>"),
        Suppression::Reasoned("cli entry point".to_string())
    );
}

#[test]
fn a_marker_above_the_imports_suppresses_the_file() {
    let source = "// bonsai-lint-ignore: route table\nimport { app } from './app';\nif (a) { app.use(b); }\n";
    assert_eq!(
        suppression_of(source, "<toplevel>"),
        Suppression::Reasoned("route table".to_string())
    );
}

#[test]
fn a_marker_above_the_first_function_belongs_to_it_not_the_file() {
    let source = "// bonsai-lint-ignore: reason\nfunction target() { if (a) { run(); } }\nif (b) { run(); }\n";
    assert_eq!(
        suppression_of(source, "target"),
        Suppression::Reasoned("reason".to_string())
    );
    assert_eq!(suppression_of(source, "<toplevel>"), Suppression::None);
}

/// A route registration binds its callback to nothing, so the callback is positional and the
/// marker above the call stays with the file, as it would above any other statement.
#[test]
fn a_marker_above_a_leading_route_call_belongs_to_the_file() {
    let source = "// bonsai-lint-ignore: route table\napp.get('/x', () => { if (a) { f(); } });\nif (b) { g(); }\n";
    assert_eq!(
        suppression_of(source, "<toplevel>"),
        Suppression::Reasoned("route table".to_string())
    );
    assert_eq!(suppression_of(source, "app.get#1"), Suppression::None);
}

#[test]
fn a_bare_marker_at_the_top_of_the_file_is_refused() {
    let source = "// bonsai-lint-ignore\nif (a) { run(); }\n";
    assert_eq!(
        suppression_of(source, "<toplevel>"),
        Suppression::MissingReason
    );
}
