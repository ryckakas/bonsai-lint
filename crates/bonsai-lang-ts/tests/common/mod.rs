#![allow(unreachable_pub, reason = "pub marks the helpers the test files call")]

use bonsai_core::{Finding, LanguageDescriptor};
use tree_sitter::Parser;

#[allow(dead_code, reason = "not every test binary calls it")]
pub fn findings_in(descriptor: &dyn LanguageDescriptor, source: &str) -> Vec<Finding> {
    let language = descriptor.compiled();
    let mut parser = Parser::new();
    parser
        .set_language(&language.ts)
        .expect("grammar should load");
    let tree = parser.parse(source, None).expect("source should parse");
    bonsai_core::analyze(&tree, source.as_bytes(), language, true)
}

#[allow(dead_code, reason = "not every test binary calls it")]
pub fn findings(source: &str) -> Vec<Finding> {
    findings_in(&bonsai_lang_ts::TSX, source)
}

#[allow(dead_code, reason = "not every test binary calls it")]
pub fn score(body: &str) -> u32 {
    let source = format!("function target() {{\n{body}\n}}\n");
    findings(&source)
        .into_iter()
        .find(|finding| finding.name == "target")
        .unwrap_or_else(|| panic!("no finding produced for:\n{source}"))
        .score
}

#[allow(dead_code, reason = "not every test binary calls it")]
pub fn assert_scores(cases: &[(&str, u32)]) {
    for (body, expected) in cases {
        assert_eq!(score(body), *expected, "scoring:\n{body}");
    }
}
