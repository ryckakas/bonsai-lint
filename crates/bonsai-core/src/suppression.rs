use tree_sitter::Node;

use crate::collect::declaration_row;
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
            if trails_previous_sibling(current, lang) {
                break;
            }
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

/// A comment on the same row as the previous sibling's end trails that sibling, not this one.
fn trails_previous_sibling(comment: Node<'_>, lang: &Language) -> bool {
    comment.prev_sibling().is_some_and(|previous| {
        lang.role(previous) != Role::Trivia
            && previous.end_position().row == comment.start_position().row
    })
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

    let row = declaration_row(node, lang);
    let mut cursor = node.walk();
    let inside = node
        .children(&mut cursor)
        .find_map(|child| on_row(child, row, src, lang));
    if inside.is_some() {
        return inside;
    }

    // A one-line declaration's trailing comment is a child in some grammars and a sibling in
    // others; both spellings must agree.
    let mut sibling = node.next_sibling();
    while let Some(current) = sibling.filter(|next| next.start_position().row == row) {
        if lang.role(current) != Role::Trivia {
            break;
        }
        if let Some(found) = current.utf8_text(src).ok().and_then(parse_marker) {
            return Some(found);
        }
        sibling = current.next_sibling();
    }
    None
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
