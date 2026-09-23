use std::collections::HashSet;

use tree_sitter::Node;

use crate::collect::declaration_row;
use crate::finding::{Suppression, SUPPRESSION_MARKER};
use crate::language::{Flags, Language, Role};

/// A marker and the id of the comment node it was read from, so a comment can only ever
/// silence one unit.
pub(crate) type Marker = (usize, Suppression);

#[must_use]
pub fn suppression(node: Node<'_>, src: &[u8], lang: &Language) -> Suppression {
    unit_marker(node, src, lang).map_or(Suppression::None, |(_, found)| found)
}

pub(crate) fn unit_marker(node: Node<'_>, src: &[u8], lang: &Language) -> Option<Marker> {
    let mut found = None;
    lang.spec
        .hooks
        .suppression_anchors(node, src, &mut |anchor| {
            if found.is_none() {
                found = leading(anchor, src, lang).or_else(|| trailing(anchor, src, lang));
            }
        });
    found
}

/// File-level code has no declaration line, so its marker lives in the comment block at the top
/// of the file, behind the open tag or shebang. A comment the first declaration already claimed
/// stays with that declaration.
pub(crate) fn toplevel_suppression(
    root: Node<'_>,
    src: &[u8],
    lang: &Language,
    claimed: &HashSet<usize>,
) -> Suppression {
    let mut cursor = root.walk();
    let found = root
        .children(&mut cursor)
        .take_while(|child| {
            let info = lang.info(child.kind_id());
            info.role == Role::Trivia || info.flags.has(Flags::PREAMBLE)
        })
        .filter(|child| lang.role(*child) == Role::Trivia && !claimed.contains(&child.id()))
        .find_map(|child| marker_of(child, src));
    found.map_or(Suppression::None, |(_, found)| found)
}

/// Walks back over the declaration's leading trivia, so the marker works both directly above the
/// declaration and inside its docblock. Attributes and decorators are stepped over; anything
/// else ends the search.
fn leading(node: Node<'_>, src: &[u8], lang: &Language) -> Option<Marker> {
    std::iter::successors(node.prev_sibling(), Node::prev_sibling)
        .take_while(|current| belongs_above(*current, lang))
        .filter(|current| lang.role(*current) == Role::Trivia)
        .find_map(|current| marker_of(current, src))
}

fn belongs_above(node: Node<'_>, lang: &Language) -> bool {
    let info = lang.info(node.kind_id());
    match info.role {
        Role::Trivia => !trails_previous_sibling(node, lang),
        _ => info.flags.has(Flags::LEADING_TRIVIA),
    }
}

fn marker_of(comment: Node<'_>, src: &[u8]) -> Option<Marker> {
    let found = comment.utf8_text(src).ok().and_then(parse_marker)?;
    Some((comment.id(), found))
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
fn trailing(node: Node<'_>, src: &[u8], lang: &Language) -> Option<Marker> {
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
        if let Some(found) = marker_of(current, src) {
            return Some(found);
        }
        sibling = current.next_sibling();
    }
    None
}

fn on_row(node: Node<'_>, row: usize, src: &[u8], lang: &Language) -> Option<Marker> {
    if node.start_position().row > row {
        return None;
    }
    if lang.role(node) == Role::Trivia && node.start_position().row == row {
        if let Some(found) = marker_of(node, src) {
            return Some(found);
        }
    }

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
