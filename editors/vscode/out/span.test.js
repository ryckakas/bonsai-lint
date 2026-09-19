"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const node_assert_1 = require("node:assert");
const node_test_1 = require("node:test");
const span_1 = require("./span");
function underlined(source, line) {
    const lines = source.split("\n");
    const span = (0, span_1.diagnosticSpan)(lines, line);
    return (lines[span.line] ?? "").slice(span.start, span.end);
}
(0, node_test_1.test)("a declaration line is underlined in full", () => {
    const src = "export function totalOutstanding(cart: Cart): number {";
    node_assert_1.strict.equal(underlined(src, 0), src);
});
(0, node_test_1.test)("the indent is not underlined", () => {
    node_assert_1.strict.equal(underlined("    public function totalOutstanding(array $lines): float", 0), "public function totalOutstanding(array $lines): float");
});
/** A zero-width range renders as no squiggle at all, so the warning must not land on a blank. */
(0, node_test_1.test)("a top-level finding on a blank line moves to the first line with content", () => {
    const src = "\n\n<?php\nif ($a) { echo 1; }";
    const span = (0, span_1.diagnosticSpan)(src.split("\n"), 0);
    node_assert_1.strict.equal(span.line, 2);
    node_assert_1.strict.ok(span.end > span.start, "span must be visible");
    node_assert_1.strict.equal(underlined(src, 0), "<?php");
});
(0, node_test_1.test)("a span is never empty, even on a file of blank lines", () => {
    const span = (0, span_1.diagnosticSpan)(["", "", ""], 0);
    node_assert_1.strict.ok(span.end > span.start, "span must be visible");
});
(0, node_test_1.test)("a line index past the end of the file is clamped", () => {
    node_assert_1.strict.equal(underlined("only line", 99), "only line");
});
//# sourceMappingURL=span.test.js.map