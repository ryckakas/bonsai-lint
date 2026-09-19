export interface Span {
  line: number;
  start: number;
  end: number;
}

/**
 * Which characters the warning underlines: the declaration line, without its indent.
 *
 * A finding covers a whole unit rather than a single token, so the line reads better than
 * hunting for the name within it — and the name is not always on the line it is reported at.
 */
export function diagnosticSpan(lines: string[], lineIndex: number): Span {
  const clamped = Math.max(0, Math.min(lineIndex, Math.max(0, lines.length - 1)));

  // A blank line has nothing to underline, and a zero-width range renders as no squiggle at
  // all — the warning would sit in the Problems panel with nothing to point at in the editor.
  // Top-level findings report line 1, which is blank often enough to matter.
  let target = clamped;
  while (target < lines.length && (lines[target] ?? "").trim() === "") {
    target += 1;
  }
  if (target >= lines.length) {
    target = clamped;
  }

  const text = lines[target] ?? "";
  const start = text.length - text.trimStart().length;

  return { line: target, start, end: Math.max(text.length, start + 1) };
}
