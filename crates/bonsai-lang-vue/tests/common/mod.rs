#![allow(unreachable_pub)]

use bonsai_core::{Extraction, Finding};
use tree_sitter::Parser;

#[allow(dead_code)]
pub fn extract(source: &str) -> Extraction {
    let extract = bonsai_lang_vue::VUE.extract.expect("vue extracts regions");
    extract(source)
}

/// Mirrors what the scan driver does: parse the whole component with the extracted ranges, so a
/// reported line is a line in the `.vue` file.
#[allow(dead_code)]
pub fn findings(source: &str) -> Vec<Finding> {
    let extraction = extract(source);
    if extraction.ranges.is_empty() {
        return Vec::new();
    }

    let mut parser = Parser::new();
    parser
        .set_language(&extraction.language.ts)
        .expect("grammar should load");
    parser
        .set_included_ranges(&extraction.ranges)
        .expect("ranges are ordered and disjoint");

    let tree = parser.parse(source, None).expect("source should parse");
    bonsai_core::analyze(&tree, source.as_bytes(), extraction.language, true)
}

#[allow(dead_code)]
pub fn find<'a>(findings: &'a [Finding], name: &str) -> &'a Finding {
    findings
        .iter()
        .find(|finding| finding.qualified_name() == name)
        .unwrap_or_else(|| {
            let seen: Vec<_> = findings.iter().map(Finding::qualified_name).collect();
            panic!("no finding named `{name}`, saw {seen:?}")
        })
}

/// The chosen dialect is observable through what parses: `<string>x` is a type assertion to the
/// TypeScript grammar and an unclosed JSX element to TSX.
#[allow(dead_code)]
pub fn parses_cleanly(source: &str) -> bool {
    let extraction = extract(source);
    let mut parser = Parser::new();
    parser
        .set_language(&extraction.language.ts)
        .expect("grammar should load");
    parser
        .set_included_ranges(&extraction.ranges)
        .expect("ranges are ordered and disjoint");
    !parser
        .parse(source, None)
        .expect("source should parse")
        .root_node()
        .has_error()
}
