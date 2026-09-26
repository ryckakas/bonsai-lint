#![allow(unreachable_pub, reason = "pub marks the helpers the test files call")]

use bonsai_core::{Finding, LanguageDescriptor};
use tree_sitter::Parser;

/// Rejects a snippet that does not parse cleanly: indentation is easy to get wrong inside a
/// Rust string, and a misparse would score something other than what the test reads.
#[allow(dead_code, reason = "not every test binary calls it")]
pub fn findings(source: &str) -> Vec<Finding> {
    let language = bonsai_lang_python::PYTHON.compiled();
    let mut parser = Parser::new();
    parser
        .set_language(&language.ts)
        .expect("Python grammar should load");
    let tree = parser.parse(source, None).expect("source should parse");
    assert!(
        !tree.root_node().has_error(),
        "unexpected parse error in:\n{source}"
    );
    bonsai_core::analyze(&tree, source.as_bytes(), language, true)
}

/// Scores a flush-left body as the body of `def target():`, indenting every line.
#[allow(dead_code, reason = "not every test binary calls it")]
pub fn score(body: &str) -> u32 {
    let mut source = String::from("def target():\n");
    for line in body.lines() {
        source.push_str("    ");
        source.push_str(line);
        source.push('\n');
    }
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

/// The score of the unit whose qualified key is `qualified`, for cases that need a class or a
/// signature of their own.
#[allow(dead_code, reason = "not every test binary calls it")]
pub fn score_of(source: &str, qualified: &str) -> u32 {
    findings(source)
        .into_iter()
        .find(|finding| finding.qualified_name() == qualified)
        .unwrap_or_else(|| panic!("no finding keyed {qualified} in:\n{source}"))
        .score
}
