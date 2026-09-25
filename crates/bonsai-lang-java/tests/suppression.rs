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
    let source = "class C {\n  // bonsai-lint-ignore: parser state machine\n  void target() { if (a) { f(); } }\n}\n";
    assert_eq!(
        suppression_of(source, "C::target()"),
        reasoned("parser state machine")
    );
}

#[test]
fn a_reasoned_marker_inside_javadoc_is_honoured() {
    let source = "class C {\n  /**\n   * Dispatches.\n   * bonsai-lint-ignore: generated dispatch table\n   */\n  void target() { if (a) { f(); } }\n}\n";
    assert_eq!(
        suppression_of(source, "C::target()"),
        reasoned("generated dispatch table")
    );
}

#[test]
fn a_marker_above_javadoc_and_annotations_is_honoured() {
    let source = "class C {\n  // bonsai-lint-ignore: framework contract\n  /** Handles it. */\n  @Override\n  @Transactional\n  public void target() { if (a) { f(); } }\n}\n";
    assert_eq!(
        suppression_of(source, "C::target()"),
        reasoned("framework contract")
    );
}

#[test]
fn a_trailing_marker_on_the_signature_line_is_honoured() {
    let source =
        "class C {\n  void target() { // bonsai-lint-ignore: legacy\n    if (a) { f(); }\n  }\n}\n";
    assert_eq!(suppression_of(source, "C::target()"), reasoned("legacy"));
}

#[test]
fn a_trailing_marker_on_an_annotated_method_is_honoured() {
    let source = "class C {\n  @GetMapping(\"/x\")\n  public void target() { // bonsai-lint-ignore: legacy\n    if (a) { f(); }\n  }\n}\n";
    assert_eq!(suppression_of(source, "C::target()"), reasoned("legacy"));
}

#[test]
fn a_bare_marker_is_refused() {
    let source = "class C {\n  // bonsai-lint-ignore\n  void target() { if (a) { f(); } }\n}\n";
    assert_eq!(
        suppression_of(source, "C::target()"),
        Suppression::MissingReason
    );

    let empty = "class C {\n  // bonsai-lint-ignore:\n  void target() { if (a) { f(); } }\n}\n";
    assert_eq!(
        suppression_of(empty, "C::target()"),
        Suppression::MissingReason
    );
}

#[test]
fn a_marker_above_a_field_lambda_is_honoured() {
    let source = "class C {\n  // bonsai-lint-ignore: hand-tuned\n  private final Runnable handler = () -> { if (a) { f(); } };\n}\n";
    assert_eq!(suppression_of(source, "C::handler"), reasoned("hand-tuned"));
}

#[test]
fn a_marker_above_a_wrapped_lambda_is_honoured() {
    let source = "class C {\n  // bonsai-lint-ignore: cached\n  static final Supplier<X> S = memoize(() -> { if (a) { return x; } return y; });\n}\n";
    assert_eq!(suppression_of(source, "C::S"), reasoned("cached"));
}

#[test]
fn a_marker_above_a_static_initializer_is_honoured() {
    let source =
        "class C {\n  // bonsai-lint-ignore: registry bootstrap\n  static { if (a) { f(); } }\n}\n";
    assert_eq!(
        suppression_of(source, "C::<static>"),
        reasoned("registry bootstrap")
    );
}

#[test]
fn an_overload_is_suppressed_on_its_own() {
    let source = "class C {\n  // bonsai-lint-ignore: legacy\n  void target(int a) { if (a > 0) { f(); } }\n  void target() { if (b) { f(); } }\n}\n";
    assert_eq!(suppression_of(source, "C::target(int)"), reasoned("legacy"));
    assert_eq!(suppression_of(source, "C::target()"), Suppression::None);
}

#[test]
fn a_marker_cannot_leak_into_the_next_declaration() {
    let source = "class C {\n  // bonsai-lint-ignore: only the first\n  void first() { if (a) { f(); } }\n  void second() { if (b) { f(); } }\n}\n";
    assert_eq!(suppression_of(source, "C::second()"), Suppression::None);
}

#[test]
fn a_trailing_marker_on_a_closing_brace_line_suppresses_nothing() {
    let source = "class C {\n  void first() {\n    if (a) { f(); }\n  } // bonsai-lint-ignore: nobody's\n  void second() { if (b) { f(); } }\n}\n";
    assert_eq!(suppression_of(source, "C::first()"), Suppression::None);
    assert_eq!(suppression_of(source, "C::second()"), Suppression::None);
}

#[test]
fn an_annotation_does_not_move_the_reported_line() {
    let source = "class C {\n  @Override\n  @Transactional(readOnly = true)\n  public void target() { if (a) { f(); } }\n}\n";
    assert_eq!(line_of(source, "C::target()"), 4);
}

/// File-level code is reported on line 1, so its marker sits in the comment block at the top of
/// the file, above or just after the package declaration.
#[test]
fn a_marker_above_the_package_declaration_suppresses_the_file() {
    let source =
        "// bonsai-lint-ignore: flag table\npackage p;\n\nclass C { static final boolean OK = a && b; }\n";
    assert_eq!(suppression_of(source, "<toplevel>"), reasoned("flag table"));
}

#[test]
fn a_marker_behind_the_package_declaration_suppresses_the_file() {
    let source =
        "package p;\n\n// bonsai-lint-ignore: flag table\n\nclass C { static final boolean OK = a && b; }\n";
    assert_eq!(suppression_of(source, "<toplevel>"), reasoned("flag table"));
}

#[test]
fn a_marker_above_the_first_method_belongs_to_it_not_the_file() {
    let source = "package p;\n\nclass C {\n  // bonsai-lint-ignore: reason\n  void target() { if (a) { f(); } }\n  static final boolean OK = a && b;\n}\n";
    assert_eq!(suppression_of(source, "C::target()"), reasoned("reason"));
    assert_eq!(suppression_of(source, "<toplevel>"), Suppression::None);
}

#[test]
fn a_bare_marker_at_the_top_of_the_file_is_refused() {
    let source =
        "// bonsai-lint-ignore\npackage p;\n\nclass C { static final boolean OK = a && b; }\n";
    assert_eq!(
        suppression_of(source, "<toplevel>"),
        Suppression::MissingReason
    );
}
