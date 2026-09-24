//! A marker without a reason is refused rather than obeyed, so silencing a finding stays a
//! documented decision.

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

#[test]
fn a_reasoned_marker_above_the_declaration_is_honoured() {
    let source = "<?php\n// bonsai-lint-ignore: parser state machine, splitting it hurts\nfunction target() { if ($a) { echo 1; } }\n";
    assert_eq!(
        suppression_of(source, "target"),
        Suppression::Reasoned("parser state machine, splitting it hurts".to_string())
    );
}

#[test]
fn a_reasoned_marker_in_a_docblock_is_honoured() {
    let source = "<?php\n/**\n * bonsai-lint-ignore: generated dispatch table\n */\nfunction target() { if ($a) { echo 1; } }\n";
    assert_eq!(
        suppression_of(source, "target"),
        Suppression::Reasoned("generated dispatch table".to_string())
    );
}

#[test]
fn a_trailing_marker_on_the_signature_line_is_honoured() {
    let source =
        "<?php\nfunction target() { // bonsai-lint-ignore: legacy\n    if ($a) { echo 1; }\n}\n";
    assert_eq!(
        suppression_of(source, "target"),
        Suppression::Reasoned("legacy".to_string())
    );
}

#[test]
fn a_bare_marker_is_refused() {
    let source = "<?php\n// bonsai-lint-ignore\nfunction target() { if ($a) { echo 1; } }\n";
    assert_eq!(suppression_of(source, "target"), Suppression::MissingReason);

    let empty = "<?php\n// bonsai-lint-ignore:\nfunction target() { if ($a) { echo 1; } }\n";
    assert_eq!(suppression_of(empty, "target"), Suppression::MissingReason);
}

#[test]
fn attributes_between_the_marker_and_the_declaration_are_stepped_over() {
    let source = "<?php\nclass A {\n// bonsai-lint-ignore: framework contract\n#[Deprecated]\npublic function target() { if ($a) { echo 1; } }\n}\n";
    assert_eq!(
        suppression_of(source, "target"),
        Suppression::Reasoned("framework contract".to_string())
    );
}

#[test]
fn a_marker_cannot_leak_into_the_next_declaration() {
    let source = "<?php\n// bonsai-lint-ignore: only the first\nfunction first() { if ($a) { echo 1; } }\nfunction second() { if ($b) { echo 1; } }\n";
    assert_eq!(suppression_of(source, "second"), Suppression::None);
}

/// On one line, the closing brace shares the signature line, so the marker is the declaration's
/// own; it must never reach the declaration below.
#[test]
fn a_trailing_marker_after_a_one_line_declaration_belongs_to_it() {
    let source = "<?php\nfunction first() { if ($a) { echo 1; } } // bonsai-lint-ignore: mine\nfunction second() { if ($b) { echo 1; } }\n";
    assert_eq!(
        suppression_of(source, "first"),
        Suppression::Reasoned("mine".to_string())
    );
    assert_eq!(suppression_of(source, "second"), Suppression::None);
}

#[test]
fn a_trailing_marker_on_a_closing_brace_line_suppresses_nothing() {
    let source = "<?php\nfunction first() {\n    if ($a) { echo 1; }\n} // bonsai-lint-ignore: nobody's\nfunction second() { if ($b) { echo 1; } }\n";
    assert_eq!(suppression_of(source, "first"), Suppression::None);
    assert_eq!(suppression_of(source, "second"), Suppression::None);
}

#[test]
fn a_trailing_marker_on_an_attributed_method_is_honoured() {
    let source = "<?php\nclass A {\n    #[Route('/x')]\n    public function target() { // bonsai-lint-ignore: legacy\n        if ($a) { echo 1; }\n    }\n}\n";
    assert_eq!(
        suppression_of(source, "target"),
        Suppression::Reasoned("legacy".to_string())
    );
}

#[test]
fn a_marker_above_an_assigned_closure_is_honoured() {
    let source = "<?php\n// bonsai-lint-ignore: hand-tuned\n$handler = function () { if ($a) { echo 1; } };\n";
    assert_eq!(
        suppression_of(source, "handler"),
        Suppression::Reasoned("hand-tuned".to_string())
    );
}

/// The namer hands a lone callback the binding of the call wrapping it, so the marker above that
/// binding must cover it too.
#[test]
fn a_marker_above_a_wrapped_closure_is_honoured() {
    let source = "<?php\n// bonsai-lint-ignore: route table\n$handler = wrap(function () { if ($a) { echo 1; } });\n";
    assert_eq!(
        suppression_of(source, "handler"),
        Suppression::Reasoned("route table".to_string())
    );
}

/// File-level code is reported on line 1, so its marker trails the open tag or sits in the
/// comment block at the top of the file.
#[test]
fn a_marker_on_the_open_tag_line_suppresses_the_file() {
    let source =
        "<?php // bonsai-lint-ignore: legacy bootstrap script\nif ($a) { if ($b) { echo 1; } }\n";
    assert_eq!(
        suppression_of(source, "<toplevel>"),
        Suppression::Reasoned("legacy bootstrap script".to_string())
    );
}

#[test]
fn a_marker_in_a_top_of_file_docblock_suppresses_the_file() {
    let source = "<?php\n/**\n * Template.\n * bonsai-lint-ignore: rendered markup\n */\nif ($a) { echo 1; }\n";
    assert_eq!(
        suppression_of(source, "<toplevel>"),
        Suppression::Reasoned("rendered markup".to_string())
    );
}

#[test]
fn a_bare_marker_on_the_open_tag_line_is_refused_for_the_file() {
    let source = "<?php // bonsai-lint-ignore\nif ($a) { echo 1; }\n";
    assert_eq!(
        suppression_of(source, "<toplevel>"),
        Suppression::MissingReason
    );
}

/// One comment silences one unit: a marker directly above the first function is that function's,
/// while one trailing the open tag can only be the file's.
#[test]
fn a_marker_above_the_first_function_belongs_to_it_not_the_file() {
    let source = "<?php\n// bonsai-lint-ignore: reason\nfunction target() { if ($a) { echo 1; } }\nif ($b) { echo 2; }\n";
    assert_eq!(
        suppression_of(source, "target"),
        Suppression::Reasoned("reason".to_string())
    );
    assert_eq!(suppression_of(source, "<toplevel>"), Suppression::None);

    let on_tag = "<?php // bonsai-lint-ignore: reason\nfunction target() { if ($a) { echo 1; } }\nif ($b) { echo 2; }\n";
    assert_eq!(suppression_of(on_tag, "target"), Suppression::None);
    assert_eq!(
        suppression_of(on_tag, "<toplevel>"),
        Suppression::Reasoned("reason".to_string())
    );
}

#[test]
fn a_marker_directly_above_a_wrapped_closure_is_honoured() {
    let source = "<?php\n$handler = wrap(\n    'x',\n    // bonsai-lint-ignore: inline\n    function () { if ($a) { echo 1; } }\n);\n";
    assert_eq!(
        suppression_of(source, "handler"),
        Suppression::Reasoned("inline".to_string())
    );
}

#[test]
fn a_marker_above_a_bound_iife_is_honoured() {
    let source = "<?php\n// bonsai-lint-ignore: startup\n$config = (function () { if ($a) { return 1; } return 2; })();\n";
    assert_eq!(
        suppression_of(source, "config"),
        Suppression::Reasoned("startup".to_string())
    );
}

/// A route registration binds its callback to nothing, so the callback is positional and the
/// marker above the call stays with the file, as it would above any other statement.
#[test]
fn a_marker_above_a_leading_route_call_belongs_to_the_file() {
    let source = "<?php\n// bonsai-lint-ignore: route table\nRoute::get('/x', function () { if ($a) { echo 1; } });\nif ($b) { echo 2; }\n";
    assert_eq!(
        suppression_of(source, "<toplevel>"),
        Suppression::Reasoned("route table".to_string())
    );
    assert_eq!(suppression_of(source, "Route::get#1"), Suppression::None);
}

#[test]
fn a_marker_after_the_first_statement_does_not_suppress_the_file() {
    let source = "<?php\n$x = 1;\n// bonsai-lint-ignore: reason\nif ($a) { echo 1; }\n";
    assert_eq!(suppression_of(source, "<toplevel>"), Suppression::None);
}
