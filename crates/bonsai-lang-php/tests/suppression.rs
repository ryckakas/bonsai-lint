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
    let source = "<?php\n// bonsai-ignore: parser state machine, splitting it hurts\nfunction target() { if ($a) { echo 1; } }\n";
    assert_eq!(
        suppression_of(source, "target"),
        Suppression::Reasoned("parser state machine, splitting it hurts".to_string())
    );
}

#[test]
fn a_reasoned_marker_in_a_docblock_is_honoured() {
    let source = "<?php\n/**\n * bonsai-ignore: generated dispatch table\n */\nfunction target() { if ($a) { echo 1; } }\n";
    assert_eq!(
        suppression_of(source, "target"),
        Suppression::Reasoned("generated dispatch table".to_string())
    );
}

#[test]
fn a_trailing_marker_on_the_signature_line_is_honoured() {
    let source =
        "<?php\nfunction target() { // bonsai-ignore: legacy\n    if ($a) { echo 1; }\n}\n";
    assert_eq!(
        suppression_of(source, "target"),
        Suppression::Reasoned("legacy".to_string())
    );
}

#[test]
fn a_bare_marker_is_refused() {
    let source = "<?php\n// bonsai-ignore\nfunction target() { if ($a) { echo 1; } }\n";
    assert_eq!(suppression_of(source, "target"), Suppression::MissingReason);

    let empty = "<?php\n// bonsai-ignore:\nfunction target() { if ($a) { echo 1; } }\n";
    assert_eq!(suppression_of(empty, "target"), Suppression::MissingReason);
}

#[test]
fn attributes_between_the_marker_and_the_declaration_are_stepped_over() {
    let source = "<?php\nclass A {\n// bonsai-ignore: framework contract\n#[Deprecated]\npublic function target() { if ($a) { echo 1; } }\n}\n";
    assert_eq!(
        suppression_of(source, "target"),
        Suppression::Reasoned("framework contract".to_string())
    );
}

#[test]
fn a_marker_cannot_leak_into_the_next_declaration() {
    let source = "<?php\n// bonsai-ignore: only the first\nfunction first() { if ($a) { echo 1; } }\nfunction second() { if ($b) { echo 1; } }\n";
    assert_eq!(suppression_of(source, "second"), Suppression::None);
}
