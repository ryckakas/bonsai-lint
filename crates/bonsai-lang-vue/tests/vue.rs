mod common;

use std::fmt::Write;

use bonsai_core::TOPLEVEL_UNIT;
use common::{find, findings};

fn component(template_lines: usize) -> String {
    let mut template = String::new();
    for n in 0..template_lines {
        writeln!(template, "  <p v-if=\"a && b\">{n}</p>").expect("string write");
    }
    format!(
        "<template>\n{template}</template>\n\n\
         <script setup lang=\"ts\">\n\
         function classify(n: number): string {{\n\
         \x20 if (n > 10) {{\n\
         \x20   if (n > 20) {{ return 'big' }}\n\
         \x20   return 'mid'\n\
         \x20 }}\n\
         \x20 return 'small'\n\
         }}\n\
         </script>\n"
    )
}

#[test]
fn a_reported_line_is_a_line_in_the_vue_file() {
    let found = findings(&component(40));
    // `<template>` + 40 lines + `</template>` + blank + `<script setup>` puts the body on 45.
    assert_eq!(find(&found, "classify").line, 45);
}

/// The property the whole design turns on: a baseline key must not drift by the length of a
/// template, and the line must track the template exactly.
#[test]
fn growing_the_template_moves_the_line_and_nothing_else() {
    let before = findings(&component(40));
    let after = findings(&component(50));

    let before = find(&before, "classify");
    let after = find(&after, "classify");

    assert_eq!(after.line - before.line, 10);
    assert_eq!(before.score, after.score);
    assert_eq!(before.qualified_name(), after.qualified_name());
}

#[test]
fn composition_api_bindings_take_their_bound_name() {
    let found = findings(
        "<script setup lang=\"ts\">\n\
         const label = computed(() => (a ? 1 : 2))\n\
         const go = () => { if (a) { b() } }\n\
         </script>\n",
    );
    find(&found, "label");
    find(&found, "go");
}

#[test]
fn a_script_setup_without_lang_is_scored_as_javascript() {
    let found =
        findings("<script setup>\nfunction f(n) { if (n) { return 1 } return 0 }\n</script>\n");
    assert_eq!(find(&found, "f").score, 1);
}

#[test]
fn options_api_methods_are_found_under_the_default_export() {
    let found = findings(
        "<script>\n\
         export default {\n\
         \x20 methods: {\n\
         \x20   increment(n) { if (n > 1) { return 2 } return 1 }\n\
         \x20 }\n\
         }\n\
         </script>\n",
    );
    assert_eq!(find(&found, "default::methods::increment").score, 1);
}

#[test]
fn two_script_blocks_produce_exactly_one_toplevel_finding() {
    let found = findings(
        "<script lang=\"ts\">\nif (a) { b() }\n</script>\n\
         <script setup lang=\"ts\">\nif (c) { d() }\n</script>\n",
    );
    let toplevel: Vec<_> = found
        .iter()
        .filter(|finding| finding.name == TOPLEVEL_UNIT)
        .collect();
    assert_eq!(toplevel.len(), 1);
}

/// Only script blocks score. Counting `v-if` would make a component incomparable with the same
/// logic written in TypeScript.
#[test]
fn template_branching_is_not_scored() {
    let found = findings(
        "<template>\n\
         \x20 <p v-if=\"a\">1</p>\n\
         \x20 <p v-else-if=\"b && c\">2</p>\n\
         \x20 <li v-for=\"x in xs\" :key=\"x\">{{ x }}</li>\n\
         </template>\n\
         <script setup lang=\"ts\">\nfunction f() { return 1 }\n</script>\n",
    );
    assert!(found.iter().all(|finding| finding.score == 0));
}

#[test]
fn the_same_logic_scores_the_same_in_a_vue_block_and_a_ts_file() {
    let body = "function f(n) { if (n > 1) { if (n > 2) { return 3 } return 2 } return 1 }";

    let vue = findings(&format!("<script setup>\n{body}\n</script>\n"));

    let language = (bonsai_lang_ts::TSX.compiled)();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language.ts).expect("grammar loads");
    let tree = parser.parse(body, None).expect("parses");
    let ts = bonsai_core::analyze(&tree, body.as_bytes(), language, true);

    assert_eq!(find(&vue, "f").score, find(&ts, "f").score);
}

#[test]
fn a_suppression_marker_inside_a_script_block_is_honoured() {
    let found = findings(
        "<template><p/></template>\n\
         <script setup lang=\"ts\">\n\
         // bonsai-lint-ignore: legacy branch table\n\
         function f(n: number) { if (n) { return 1 } return 0 }\n\
         </script>\n",
    );
    assert!(find(&found, "f").is_suppressed());
}
