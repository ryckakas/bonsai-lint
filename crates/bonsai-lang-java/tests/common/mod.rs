#![allow(unreachable_pub)]

use bonsai_core::{Finding, LanguageDescriptor};
use tree_sitter::Parser;

#[allow(dead_code)]
pub fn findings(source: &str) -> Vec<Finding> {
    let language = bonsai_lang_java::JAVA.compiled();
    let mut parser = Parser::new();
    parser
        .set_language(&language.ts)
        .expect("Java grammar should load");
    let tree = parser.parse(source, None).expect("source should parse");
    assert!(
        !tree.root_node().has_error(),
        "fixture does not parse cleanly:\n{source}"
    );
    bonsai_core::analyze(&tree, source.as_bytes(), language, true)
}

#[allow(dead_code)]
pub fn score(body: &str) -> u32 {
    let source = format!("class T {{\n    void target() {{\n{body}\n    }}\n}}\n");
    findings(&source)
        .into_iter()
        .find(|finding| finding.name == "target()")
        .unwrap_or_else(|| panic!("no finding produced for:\n{source}"))
        .score
}

#[allow(dead_code)]
pub fn assert_scores(cases: &[(&str, u32)]) {
    for (body, expected) in cases {
        assert_eq!(score(body), *expected, "scoring:\n{body}");
    }
}

/// The score of the unit whose qualified key is `qualified`, for cases that need a signature or
/// a class of their own.
#[allow(dead_code)]
pub fn score_of(source: &str, qualified: &str) -> u32 {
    findings(source)
        .into_iter()
        .find(|finding| finding.qualified_name() == qualified)
        .unwrap_or_else(|| panic!("no finding keyed {qualified} in:\n{source}"))
        .score
}
