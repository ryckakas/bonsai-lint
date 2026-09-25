#![allow(unreachable_pub, reason = "pub marks the helpers the test files call")]

use bonsai_core::{Extraction, Finding, LanguageDescriptor};
use tree_sitter::Parser;

#[allow(dead_code, reason = "not every test binary calls it")]
pub fn extract(source: &str) -> Extraction {
    bonsai_lang_vue::VUE
        .extract(source)
        .expect("vue extracts regions")
}

/// Mirrors what the scan driver does: one parse per region over the whole component, so a
/// reported line is a line in the `.vue` file and a comment cannot run from one block into the
/// next. The driver then merges the regions' top-level findings; these tests assert on that too.
#[allow(dead_code, reason = "not every test binary calls it")]
pub fn findings(source: &str) -> Vec<Finding> {
    let extraction = extract(source);
    let mut parser = Parser::new();
    parser
        .set_language(&extraction.language.ts)
        .expect("grammar should load");

    let mut found = Vec::new();
    for range in &extraction.ranges {
        parser
            .set_included_ranges(std::slice::from_ref(range))
            .expect("ranges are ordered and disjoint");
        let tree = parser.parse(source, None).expect("source should parse");
        found.extend(bonsai_core::analyze(
            &tree,
            source.as_bytes(),
            extraction.language,
            true,
        ));
    }
    merge_toplevel(&mut found);
    bonsai_core::disambiguate(&mut found);
    found
}

#[allow(dead_code, reason = "not every test binary calls it")]
fn merge_toplevel(findings: &mut Vec<Finding>) {
    let is_toplevel = |finding: &Finding| finding.name == bonsai_core::TOPLEVEL_UNIT;
    let Some(first) = findings.iter().position(is_toplevel) else {
        return;
    };
    findings[first].score = findings
        .iter()
        .filter(|f| is_toplevel(f))
        .map(|f| f.score)
        .sum();
    let mut kept = false;
    findings.retain(|finding| {
        if !is_toplevel(finding) {
            return true;
        }
        let keep = !kept;
        kept = true;
        keep
    });
}

#[allow(dead_code, reason = "not every test binary calls it")]
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
#[allow(dead_code, reason = "not every test binary calls it")]
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
