import { strict as assert } from "node:assert";
import { test } from "node:test";
import { diagnosticSpan } from "./span";

function underlined(source: string, line: number): string {
  const lines = source.split("\n");
  const span = diagnosticSpan(lines, line);
  return (lines[span.line] ?? "").slice(span.start, span.end);
}

test("a declaration line is underlined in full", () => {
  const src = "export function totalOutstanding(cart: Cart): number {";
  assert.equal(underlined(src, 0), src);
});

test("the indent is not underlined", () => {
  assert.equal(
    underlined("    public function totalOutstanding(array $lines): float", 0),
    "public function totalOutstanding(array $lines): float",
  );
});

/** A zero-width range renders as no squiggle at all, so the warning must not land on a blank. */
test("a top-level finding on a blank line moves to the first line with content", () => {
  const src = "\n\n<?php\nif ($a) { echo 1; }";
  const span = diagnosticSpan(src.split("\n"), 0);
  assert.equal(span.line, 2);
  assert.ok(span.end > span.start, "span must be visible");
  assert.equal(underlined(src, 0), "<?php");
});

test("a span is never empty, even on a file of blank lines", () => {
  const span = diagnosticSpan(["", "", ""], 0);
  assert.ok(span.end > span.start, "span must be visible");
});

test("a line index past the end of the file is clamped", () => {
  assert.equal(underlined("only line", 99), "only line");
});
