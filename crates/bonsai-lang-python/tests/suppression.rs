//! A marker without a reason is refused rather than obeyed, so silencing a finding stays a
//! documented decision. Every binding shape the namer knows must also be suppressible.

mod common;

use bonsai_core::Suppression;
use common::findings;

fn suppression_of(source: &str, qualified: &str) -> Suppression {
    findings(source)
        .into_iter()
        .find(|finding| finding.qualified_name() == qualified)
        .unwrap_or_else(|| panic!("no finding keyed {qualified} in:\n{source}"))
        .suppression
}

fn line_of(source: &str, qualified: &str) -> usize {
    findings(source)
        .into_iter()
        .find(|finding| finding.qualified_name() == qualified)
        .unwrap_or_else(|| panic!("no finding keyed {qualified} in:\n{source}"))
        .line
}

fn reasoned(reason: &str) -> Suppression {
    Suppression::Reasoned(reason.to_string())
}

#[test]
fn a_reasoned_marker_above_the_declaration_is_honoured() {
    let source =
        "# bonsai-lint-ignore: parser state machine\ndef target():\n    if a:\n        f()\n";
    assert_eq!(
        suppression_of(source, "target"),
        reasoned("parser state machine")
    );
}

#[test]
fn a_marker_above_the_decorators_is_honoured() {
    let source = "# bonsai-lint-ignore: framework contract\n@app.route(\"/\")\n@login_required\ndef target():\n    if a:\n        f()\n";
    assert_eq!(
        suppression_of(source, "target"),
        reasoned("framework contract")
    );
}

#[test]
fn a_marker_between_decorators_is_honoured() {
    let source = "@app.route(\"/\")\n# bonsai-lint-ignore: legacy\n@login_required\ndef target():\n    if a:\n        f()\n";
    assert_eq!(suppression_of(source, "target"), reasoned("legacy"));
}

#[test]
fn a_trailing_marker_on_the_signature_line_is_honoured() {
    let source = "def target():  # bonsai-lint-ignore: legacy\n    if a:\n        f()\n";
    assert_eq!(suppression_of(source, "target"), reasoned("legacy"));
}

#[test]
fn a_trailing_marker_on_a_decorated_function_is_honoured() {
    let source = "@cache\ndef target():  # bonsai-lint-ignore: legacy\n    if a:\n        f()\n";
    assert_eq!(suppression_of(source, "target"), reasoned("legacy"));
}

#[test]
fn a_marker_above_a_method_is_honoured() {
    let source = "class C:\n    # bonsai-lint-ignore: legacy\n    @property\n    def target(self):\n        if a:\n            f()\n";
    assert_eq!(suppression_of(source, "C::target"), reasoned("legacy"));
}

#[test]
fn a_bare_marker_is_refused() {
    let source = "# bonsai-lint-ignore\ndef target():\n    if a:\n        f()\n";
    assert_eq!(suppression_of(source, "target"), Suppression::MissingReason);

    let empty = "# bonsai-lint-ignore:\ndef target():\n    if a:\n        f()\n";
    assert_eq!(suppression_of(empty, "target"), Suppression::MissingReason);
}

#[test]
fn a_marker_above_or_behind_a_bound_lambda_is_honoured() {
    let above = "# bonsai-lint-ignore: hand-tuned\nhandler = lambda e: 1 if e else 2\n";
    assert_eq!(suppression_of(above, "handler"), reasoned("hand-tuned"));

    let behind = "handler = lambda e: 1 if e else 2  # bonsai-lint-ignore: hand-tuned\n";
    assert_eq!(suppression_of(behind, "handler"), reasoned("hand-tuned"));

    let attribute =
        "class C:\n    # bonsai-lint-ignore: hand-tuned\n    key = lambda self: 1 if self else 2\n";
    assert_eq!(suppression_of(attribute, "C::key"), reasoned("hand-tuned"));
}

#[test]
fn a_marker_above_a_wrapped_lambda_is_honoured() {
    let source = "# bonsai-lint-ignore: cached\nS = memoize(lambda: 1 if a else 2)\n";
    assert_eq!(suppression_of(source, "S"), reasoned("cached"));
}

#[test]
fn a_marker_cannot_leak_into_the_next_declaration() {
    let source = "# bonsai-lint-ignore: only the first\ndef first():\n    if a:\n        f()\n\ndef second():\n    if b:\n        f()\n";
    assert_eq!(suppression_of(source, "second"), Suppression::None);

    let in_class = "class C:\n    # bonsai-lint-ignore: only the first\n    def first(self):\n        if a:\n            f()\n\n    def second(self):\n        if b:\n            f()\n";
    assert_eq!(
        suppression_of(in_class, "C::first"),
        reasoned("only the first")
    );
    assert_eq!(suppression_of(in_class, "C::second"), Suppression::None);

    let above_attribute = "class C:\n    # bonsai-lint-ignore: the attribute's\n    LIMIT = 3\n\n    def target(self):\n        if a:\n            f()\n";
    assert_eq!(
        suppression_of(above_attribute, "C::target"),
        Suppression::None
    );
}

/// Python has no closing brace, so the nearest shape is a marker trailing the last line of a
/// body: it belongs to that line, not to either function.
#[test]
fn a_trailing_marker_on_the_last_body_line_suppresses_nothing() {
    let source = "def first():\n    if a:\n        f()  # bonsai-lint-ignore: nobody's\ndef second():\n    if b:\n        f()\n";
    assert_eq!(suppression_of(source, "first"), Suppression::None);
    assert_eq!(suppression_of(source, "second"), Suppression::None);
}

/// A docstring is a string in the body, not a comment, so it cannot carry a marker.
#[test]
fn a_marker_in_a_docstring_is_not_a_marker() {
    let source = "def target():\n    \"\"\"bonsai-lint-ignore: not a comment\"\"\"\n    if a:\n        f()\n";
    assert_eq!(suppression_of(source, "target"), Suppression::None);
}

#[test]
fn a_decorator_does_not_move_the_reported_line() {
    let source = "@app.route(\"/\")\n@login_required\ndef target():\n    if a:\n        f()\n";
    assert_eq!(line_of(source, "target"), 3);
}

/// File-level code is reported on line 1, so its marker sits in the comment block at the top of
/// the file: behind a shebang or coding line, and above the module docstring.
#[test]
fn a_marker_at_the_top_of_the_file_suppresses_the_file() {
    let first_line = "# bonsai-lint-ignore: flag table\n\nFLAG = a and b\n";
    assert_eq!(
        suppression_of(first_line, "<toplevel>"),
        reasoned("flag table")
    );

    let behind_shebang = "#!/usr/bin/env python\n# -*- coding: utf-8 -*-\n# bonsai-lint-ignore: flag table\n\nFLAG = a and b\n";
    assert_eq!(
        suppression_of(behind_shebang, "<toplevel>"),
        reasoned("flag table")
    );

    let above_docstring =
        "# bonsai-lint-ignore: flag table\n\"\"\"Flags.\"\"\"\n\nFLAG = a and b\n";
    assert_eq!(
        suppression_of(above_docstring, "<toplevel>"),
        reasoned("flag table")
    );
}

#[test]
fn a_marker_above_the_first_function_belongs_to_it_not_the_file() {
    let source =
        "# bonsai-lint-ignore: reason\ndef target():\n    if a:\n        f()\n\nFLAG = a and b\n";
    assert_eq!(suppression_of(source, "target"), reasoned("reason"));
    assert_eq!(suppression_of(source, "<toplevel>"), Suppression::None);
}

#[test]
fn a_bare_marker_at_the_top_of_the_file_is_refused() {
    let source = "# bonsai-lint-ignore\n\nFLAG = a and b\n";
    assert_eq!(
        suppression_of(source, "<toplevel>"),
        Suppression::MissingReason
    );
}
