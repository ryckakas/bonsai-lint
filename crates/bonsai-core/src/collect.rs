use std::collections::HashMap;

use tree_sitter::{Node, Tree};

use crate::finding::{Finding, NameOrigin, Suppression, TOPLEVEL_UNIT};
use crate::language::{field, Flags, Language, Role};
use crate::suppression::suppression;
use crate::walk::{score_node, score_nodes, WalkCx};

#[must_use]
pub fn analyze(tree: &Tree, src: &[u8], lang: &Language, toplevel: bool) -> Vec<Finding> {
    let mut findings = Vec::new();
    let root = tree.root_node();

    collect(root, src, lang, None, &mut findings);

    if toplevel {
        if let Some(finding) = toplevel_finding(root, src, lang) {
            findings.push(finding);
        }
    }

    disambiguate(&mut findings);
    findings
}

/// Recurses top-down and stops descending at a unit, which is what implements the rule that a
/// function-like is a unit only when no other function-like encloses it. Everything nested
/// inside rolls up into that unit's score instead.
fn collect(
    node: Node<'_>,
    src: &[u8],
    lang: &Language,
    container: Option<&str>,
    findings: &mut Vec<Finding>,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let info = lang.info(child.kind_id());

        if info.flags.has(Flags::UNIT) {
            // A bodyless (abstract or interface) method would only ever report a zero.
            if field(child, lang.fields.body).is_some() {
                findings.push(score_unit(child, src, lang, container));
            }
        } else if info.flags.has(Flags::CONTAINER) {
            let nested = container_path(child, src, lang, container);
            collect(child, src, lang, nested.as_deref(), findings);
        } else {
            collect(child, src, lang, container, findings);
        }
    }
}

fn container_path(
    node: Node<'_>,
    src: &[u8],
    lang: &Language,
    outer: Option<&str>,
) -> Option<String> {
    match ((lang.spec.hooks.container_name)(node, src), outer) {
        (Some(name), Some(outer)) => Some(format!("{outer}::{name}")),
        (Some(name), None) => Some(name),
        (None, outer) => outer.map(ToString::to_string),
    }
}

fn score_unit(node: Node<'_>, src: &[u8], lang: &Language, container: Option<&str>) -> Finding {
    let name = (lang.spec.hooks.unit_name)(node, src);

    let score = {
        let cx = WalkCx {
            lang,
            src,
            unit: &name.text,
            container,
            skip_units: false,
        };
        field(node, lang.fields.body).map_or(0, |body| score_node(body, &cx))
    };

    Finding {
        container: container.map(ToString::to_string),
        name: name.text,
        origin: name.origin,
        line: declaration_row(node, lang) + 1,
        score,
        suppression: suppression(node, src, lang),
        language: lang.spec.id,
    }
}

/// A declaration node starts at its attributes or decorators, not at its signature line.
#[must_use]
pub fn declaration_row(node: Node<'_>, lang: &Language) -> usize {
    let mut cursor = node.walk();
    let first = node.children(&mut cursor).find(|child| {
        let info = lang.info(child.kind_id());
        info.role != Role::Trivia && !info.flags.has(Flags::LEADING_TRIVIA)
    });
    first.map_or(node.start_position().row, |child| {
        child.start_position().row
    })
}

/// Code outside any function is invisible to a purely unit-based scan, which is exactly where
/// procedural scripts and module-level initialisation hide. A zero score is not reported, since
/// most files legitimately have no top-level logic and emitting them all would be noise.
fn toplevel_finding(root: Node<'_>, src: &[u8], lang: &Language) -> Option<Finding> {
    let cx = WalkCx {
        lang,
        src,
        unit: TOPLEVEL_UNIT,
        container: None,
        skip_units: true,
    };

    let mut cursor = root.walk();
    let score = score_nodes(root.named_children(&mut cursor), &cx);

    if score == 0 {
        return None;
    }

    Some(Finding {
        container: None,
        name: TOPLEVEL_UNIT.to_string(),
        origin: NameOrigin::TopLevel,
        line: 1,
        score,
        suppression: Suppression::None,
        language: lang.spec.id,
    })
}

/// Only positional and anonymous names can collide in a way the author did not choose. Declared
/// and bound names that collide are a genuine duplicate in the source, which is the author's
/// problem rather than the namer's.
fn disambiguate(findings: &mut [Finding]) {
    let mut counts: HashMap<(Option<String>, String), usize> = HashMap::new();

    for finding in findings.iter_mut() {
        if !matches!(
            finding.origin,
            NameOrigin::Positional | NameOrigin::Anonymous
        ) {
            continue;
        }

        let seen = counts
            .entry((finding.container.clone(), finding.name.clone()))
            .or_insert(0);
        *seen += 1;

        if *seen > 1 {
            finding.name = format!("{}~{}", finding.name, seen);
        }
    }
}
