use tree_sitter::Node;

use crate::finding::{Suppression, SUPPRESSION_MARKER};
use crate::language::{Flags, Language, Role};

#[must_use]
pub fn suppression(node: Node<'_>, src: &[u8], lang: &Language) -> Suppression {
    let anchor = (lang.spec.hooks.suppression_anchor)(node);
    leading(anchor, src, lang)
        .or_else(|| trailing(anchor, src, lang))
        .unwrap_or(Suppression::None)
}

/// Walks back over the declaration's leading trivia, so the marker works both directly above the
/// declaration and inside its docblock. Attributes and decorators are stepped over; anything
/// else ends the search.
fn leading(node: Node<'_>, src: &[u8], lang: &Language) -> Option<Suppression> {
    let mut sibling = node.prev_sibling();

    while let Some(current) = sibling {
        let info = lang.info(current.kind_id());

        if info.role == Role::Trivia {
            if let Some(found) = current.utf8_text(src).ok().and_then(parse_marker) {
                return Some(found);
            }
        } else if !info.flags.has(Flags::LEADING_TRIVIA) {
            break;
        }

        sibling = current.prev_sibling();
    }

    None
}

/// The signature line itself, which is where `eslint-disable-line` and `phpcs:ignore` have
/// taught people to reach. Descent stops at the first node starting on a later row, so the body
/// is never searched and a marker cannot leak out of its own declaration.
fn trailing(node: Node<'_>, src: &[u8], lang: &Language) -> Option<Suppression> {
    fn on_row(node: Node<'_>, row: usize, src: &[u8], lang: &Language) -> Option<Suppression> {
        if node.start_position().row > row {
            return None;
        }

        if lang.role(node) == Role::Trivia && node.start_position().row == row {
            if let Some(found) = node.utf8_text(src).ok().and_then(parse_marker) {
                return Some(found);
            }
        }

        let mut cursor = node.walk();
        let found = node
            .children(&mut cursor)
            .find_map(|child| on_row(child, row, src, lang));
        found
    }

    let row = node.start_position().row;
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .find_map(|child| on_row(child, row, src, lang));
    found
}

fn parse_marker(comment: &str) -> Option<Suppression> {
    let after_marker = comment.split_once(SUPPRESSION_MARKER)?.1;

    let Some(rest) = after_marker.strip_prefix(':') else {
        return Some(Suppression::MissingReason);
    };

    let reason = rest
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .trim_end_matches("*/")
        .trim();

    if reason.is_empty() {
        Some(Suppression::MissingReason)
    } else {
        Some(Suppression::Reasoned(reason.to_string()))
    }
}
