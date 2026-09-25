#![allow(unreachable_pub, reason = "pub marks the helpers the test files call")]

use bonsai_core::{Finding, LanguageDescriptor};
use tree_sitter::Parser;

#[allow(dead_code, reason = "not every test binary calls it")]
pub fn findings(source: &str) -> Vec<Finding> {
    parse(source, false)
}

/// For Java the grammar cannot parse yet. It insists on the error, so a grammar upgrade that
/// learns the syntax fails the test and the workaround it pins can be retired.
#[allow(dead_code, reason = "not every test binary calls it")]
pub fn findings_in_a_grammar_gap(source: &str) -> Vec<Finding> {
    parse(source, true)
}

fn parse(source: &str, gap: bool) -> Vec<Finding> {
    let language = bonsai_lang_java::JAVA.compiled();
    let mut parser = Parser::new();
    parser
        .set_language(&language.ts)
        .expect("Java grammar should load");
    let tree = parser.parse(source, None).expect("source should parse");
    assert_eq!(
        tree.root_node().has_error(),
        gap,
        "unexpected parse for:\n{source}"
    );
    bonsai_core::analyze(&tree, source.as_bytes(), language, true)
}

#[allow(dead_code, reason = "not every test binary calls it")]
pub fn score(body: &str) -> u32 {
    let source = format!("class T {{\n    void target() {{\n{body}\n    }}\n}}\n");
    findings(&source)
        .into_iter()
        .find(|finding| finding.name == "target()")
        .unwrap_or_else(|| panic!("no finding produced for:\n{source}"))
        .score
}

#[allow(dead_code, reason = "not every test binary calls it")]
pub fn assert_scores(cases: &[(&str, u32)]) {
    for (body, expected) in cases {
        assert_eq!(score(body), *expected, "scoring:\n{body}");
    }
}

/// The score of the unit whose qualified key is `qualified`, for cases that need a signature or
/// a class of their own.
#[allow(dead_code, reason = "not every test binary calls it")]
pub fn score_of(source: &str, qualified: &str) -> u32 {
    findings(source)
        .into_iter()
        .find(|finding| finding.qualified_name() == qualified)
        .unwrap_or_else(|| panic!("no finding keyed {qualified} in:\n{source}"))
        .score
}
