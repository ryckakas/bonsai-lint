#![allow(unreachable_pub, reason = "pub marks the helpers the test files call")]

use bonsai_core::{Finding, LanguageDescriptor};
use tree_sitter::Parser;

#[allow(dead_code, reason = "not every test binary calls it")]
pub fn findings(source: &str) -> Vec<Finding> {
    let language = bonsai_lang_go::GO.compiled();
    let mut parser = Parser::new();
    parser
        .set_language(&language.ts)
        .expect("Go grammar should load");
    let tree = parser.parse(source, None).expect("source should parse");
    assert!(
        !tree.root_node().has_error(),
        "fixture does not parse cleanly:\n{source}"
    );
    bonsai_core::analyze(&tree, source.as_bytes(), language, true)
}

#[allow(dead_code, reason = "not every test binary calls it")]
pub fn score(body: &str) -> u32 {
    let source = format!("package p\n\nfunc target() {{\n{body}\n}}\n");
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
