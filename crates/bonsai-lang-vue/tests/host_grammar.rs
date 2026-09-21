//! The HTML grammar has no `LanguageSpec`, so nothing fails compilation when an upgrade renames a
//! node. Without this, extraction would quietly find no blocks and every component would score
//! zero.

use std::collections::HashSet;

use tree_sitter::{Node, Parser};

const COMPONENT: &str = r#"<template>
  <div><template #cell="{ row }"><span v-if="row.ok">{{ row.id }}</span></template></div>
</template>
<script setup lang="ts">
const a = 1
</script>
<style scoped>.a { color: red; }</style>
"#;

fn kinds(node: Node<'_>, seen: &mut HashSet<String>) {
    seen.insert(node.kind().to_string());
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        kinds(child, seen);
    }
}

#[test]
fn the_html_grammar_still_produces_every_kind_the_extractor_reads() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_html::LANGUAGE.into())
        .expect("html grammar loads");
    let tree = parser.parse(COMPONENT, None).expect("component parses");

    let mut seen = HashSet::new();
    kinds(tree.root_node(), &mut seen);

    for kind in [
        "element",
        "script_element",
        "start_tag",
        "tag_name",
        "attribute",
        "attribute_name",
        "attribute_value",
        "quoted_attribute_value",
        "raw_text",
    ] {
        assert!(
            seen.contains(kind),
            "html grammar no longer produces `{kind}`"
        );
    }
}

#[test]
fn a_realistic_vue_template_parses_without_error() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_html::LANGUAGE.into())
        .expect("html grammar loads");
    let tree = parser.parse(COMPONENT, None).expect("component parses");
    assert!(!tree.root_node().has_error());
}
