use std::cell::RefCell;

use bonsai_core::Extraction;
use tree_sitter::{Node, Parser, Range};

use crate::{compiled_tsx, compiled_typescript};

thread_local! {
    static PARSER: RefCell<Parser> = RefCell::new(html_parser());
}

struct Block {
    range: Range,
    typescript: bool,
}

pub(crate) fn extract(source: &str) -> Extraction {
    let (blocks, found) = PARSER.with_borrow_mut(|parser| {
        parser
            .parse(source, None)
            .map_or_else(Default::default, |tree| {
                blocks(tree.root_node(), source.as_bytes())
            })
    });

    let typescript = blocks.iter().any(|block| block.typescript);
    let ranges: Vec<Range> = blocks.into_iter().map(|block| block.range).collect();
    // A block skipped for `src=` is not a failure, so this counts what the grammar saw rather
    // than what survived.
    let warning = (found == 0 && source.contains("<script"))
        .then(|| "contains <script but no script block could be read".to_string());

    Extraction {
        language: if typescript {
            compiled_typescript()
        } else {
            compiled_tsx()
        },
        ranges,
        warning,
    }
}

fn html_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_html::LANGUAGE.into())
        .expect("html grammar loads");
    parser
}

fn blocks(root: Node<'_>, src: &[u8]) -> (Vec<Block>, usize) {
    let mut scripts = Vec::new();
    collect_scripts(root, src, &mut scripts);
    let found = scripts.len();
    let blocks = scripts
        .into_iter()
        .filter_map(|script| block(script, src))
        .collect();
    (blocks, found)
}

/// A pre-order walk yields the blocks in document order, which is what `set_included_ranges`
/// requires. Ancestors are checked rather than the parent alone so that an `ERROR` node produced
/// by template syntax the HTML grammar dislikes cannot hide a script block.
fn collect_scripts<'t>(node: Node<'t>, src: &[u8], out: &mut Vec<Node<'t>>) {
    if node.kind() == "script_element" {
        if !inside_template(node, src) {
            out.push(node);
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_scripts(child, src, out);
    }
}

fn inside_template(node: Node<'_>, src: &[u8]) -> bool {
    let mut parent = node.parent();
    while let Some(ancestor) = parent {
        if ancestor.kind() == "element" && tag_name(ancestor, src) == Some("template") {
            return true;
        }
        parent = ancestor.parent();
    }
    false
}

fn block(script: Node<'_>, src: &[u8]) -> Option<Block> {
    let start = named_child(script, "start_tag")?;
    let attributes = attributes(start, src);
    if attributes.iter().any(|(name, _)| *name == "src") {
        return None;
    }

    let body = named_child(script, "raw_text")?;
    let lang = attributes
        .iter()
        .find(|(name, _)| *name == "lang")
        .and_then(|(_, value)| *value);

    Some(Block {
        range: Range {
            start_byte: body.start_byte(),
            end_byte: body.end_byte(),
            start_point: body.start_position(),
            end_point: body.end_position(),
        },
        typescript: matches!(lang, Some("ts" | "mts" | "cts")),
    })
}

fn attributes<'a>(start: Node<'_>, src: &'a [u8]) -> Vec<(&'a str, Option<&'a str>)> {
    let mut cursor = start.walk();
    let found = start
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "attribute")
        .filter_map(|attribute| {
            let name = named_child(attribute, "attribute_name")?
                .utf8_text(src)
                .ok()?;
            Some((name, attribute_value(attribute, src)))
        })
        .collect();
    found
}

fn attribute_value<'a>(attribute: Node<'_>, src: &'a [u8]) -> Option<&'a str> {
    let value = match named_child(attribute, "quoted_attribute_value") {
        Some(quoted) => named_child(quoted, "attribute_value")?,
        None => named_child(attribute, "attribute_value")?,
    };
    value.utf8_text(src).ok()
}

fn tag_name<'a>(element: Node<'_>, src: &'a [u8]) -> Option<&'a str> {
    let start = named_child(element, "start_tag")?;
    named_child(start, "tag_name")?.utf8_text(src).ok()
}

fn named_child<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == kind);
    found
}
