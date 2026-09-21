mod common;

use common::{extract, parses_cleanly};

#[test]
fn a_plain_script_block_is_found() {
    assert_eq!(
        extract("<script>\nconst a = 1\n</script>\n").ranges.len(),
        1
    );
}

#[test]
fn a_script_setup_block_is_found() {
    assert_eq!(
        extract("<template><p/></template>\n<script setup>\nconst a = 1\n</script>\n")
            .ranges
            .len(),
        1
    );
}

/// Both blocks feed one parse. Scoring them separately would emit two `<toplevel>` findings whose
/// qualified names collide, and a baseline key cannot distinguish them.
#[test]
fn both_script_blocks_are_extracted_as_one_ordered_pair() {
    let source = "<script lang=\"ts\">\nexport default {}\n</script>\n\
                  <script setup lang=\"ts\">\nconst a = 1\n</script>\n";
    let ranges = extract(source).ranges;
    assert_eq!(ranges.len(), 2);
    assert!(ranges[0].end_byte <= ranges[1].start_byte);
}

#[test]
fn a_ts_block_selects_the_typescript_grammar() {
    assert!(parses_cleanly(
        "<script setup lang=\"ts\">\nconst x = <string>value\n</script>\n"
    ));
}

#[test]
fn a_block_without_lang_selects_the_jsx_capable_grammar() {
    assert!(parses_cleanly(
        "<script setup>\nconst el = <div>hi</div>\n</script>\n"
    ));
}

#[test]
fn attribute_order_and_quoting_do_not_matter() {
    for open in [
        "<script setup lang=\"ts\">",
        "<script lang='ts' setup>",
        "<script  setup  lang = \"ts\" >",
    ] {
        assert_eq!(
            extract(&format!("{open}\nconst a = 1\n</script>\n"))
                .ranges
                .len(),
            1,
            "{open}"
        );
    }
}

#[test]
fn a_block_with_src_has_no_inline_body_and_is_not_a_failure() {
    let extraction = extract("<script src=\"./x.ts\"></script>\n<template><p/></template>\n");
    assert!(extraction.ranges.is_empty());
    assert!(extraction.warning.is_none());
}

#[test]
fn a_template_only_component_scores_nothing_quietly() {
    let extraction = extract("<template><div>only</div></template>\n");
    assert!(extraction.ranges.is_empty());
    assert!(extraction.warning.is_none());
}

/// The case that ruled out a hand-rolled scanner: a `<script>` in template text is not a block.
#[test]
fn a_script_inside_the_template_is_not_a_block() {
    let source = "<template>\n  <pre><script>alert(1)</script></pre>\n</template>\n\
                  <script setup>\nconst a = 1\n</script>\n";
    let ranges = extract(source).ranges;
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].start_point.row, 3);
}

#[test]
fn a_nested_slot_template_does_not_hide_the_real_block() {
    let source = concat!(
        "<template>\n",
        "  <MyTable>\n",
        "    <template #cell=\"{ row }\">\n",
        "      <span>{{ row.id }}</span>\n",
        "    </template>\n",
        "  </MyTable>\n",
        "</template>\n",
        "<script setup lang=\"ts\">\nconst a = 1\n</script>\n",
    );
    assert_eq!(extract(source).ranges.len(), 1);
}

/// The host grammar cannot fail spec compilation, so a version that stopped finding blocks would
/// otherwise report zero findings in silence.
#[test]
fn text_that_looks_like_a_script_but_yields_no_block_warns() {
    let extraction = extract("<template><p/></template>\n<!-- <script setup> was here -->\n");
    assert!(extraction.ranges.is_empty());
    assert!(extraction.warning.is_some());
}

#[test]
fn crlf_line_endings_keep_the_row_correct() {
    let source = "<template>\r\n<p/>\r\n</template>\r\n<script setup lang=\"ts\">\r\nconst a = 1\r\n</script>\r\n";
    assert_eq!(extract(source).ranges[0].start_point.row, 3);
}

/// An unclosed `<template>` reparents the script under an `ERROR` node, two levels down. Matching
/// on the root's direct children would lose the block and score the component zero in silence.
#[test]
fn an_unclosed_template_does_not_hide_the_script_block() {
    let extraction = extract("<template><div>\n<script setup>\nconst a = 1\n</script>\n");
    assert_eq!(extraction.ranges.len(), 1);
    assert!(extraction.warning.is_none());
}

#[test]
fn an_unreadable_component_warns_rather_than_scoring_zero() {
    let extraction =
        extract("<template><p class=\"x></p></template>\n<script setup>\nconst a = 1\n</script>\n");
    assert!(extraction.ranges.is_empty());
    assert!(extraction.warning.is_some());
}

#[test]
fn a_byte_order_mark_keeps_the_row_correct() {
    let source =
        "\u{feff}<template><p/></template>\n<script setup lang=\"ts\">\nconst a = 1\n</script>\n";
    assert_eq!(extract(source).ranges[0].start_point.row, 1);
}
