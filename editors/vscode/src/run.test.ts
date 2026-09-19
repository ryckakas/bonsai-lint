import { strict as assert } from "node:assert";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { scan, type ScanOptions } from "./run";
import type { Binary } from "./binary";

const scripts = mkdtempSync(join(tmpdir(), "bonsai-lint-fake-cli-"));
let counter = 0;

/**
 * A stand-in for the CLI. It has to be a script file rather than `node -e`, because node reads
 * the trailing `--format json` as its own option and refuses to start.
 */
function fakeCli(script: string): Binary {
  const file = join(scripts, `fake-${(counter += 1)}.js`);
  writeFileSync(file, script);
  return { command: process.execPath, prefixArgs: [file], source: "path" };
}

/** Echoes the arguments it was given and the text it was piped, as one finding. */
const echoer = () =>
  fakeCli(`
    let input = "";
    process.stdin.on("data", (chunk) => { input += chunk; });
    process.stdin.on("end", () => {
      console.log(JSON.stringify({
        thresholds: {}, breaches: 0,
        findings: [{
          path: process.argv.slice(2).join(" "),
          line: 1, name: input, score: 1, language: "php", domain: "root",
        }],
      }));
    });
  `);

function run(binary: Binary, overrides: Partial<ScanOptions> = {}) {
  return scan({
    binary,
    file: "any.php",
    workspaceRoot: process.cwd(),
    text: "",
    ...overrides,
  });
}

test("the buffer is piped in rather than read from disk", async () => {
  const [finding] = await run(echoer(), { text: "<?php // unsaved" });
  assert.equal(finding.name, "<?php // unsaved");
});

test("the file is passed as --stdin-path so the language can be chosen", async () => {
  const [finding] = await run(echoer(), { file: "/repo/src/Thing.php" });
  assert.match(finding.path, /--stdin --stdin-path \/repo\/src\/Thing\.php/);
});

test("--over is withheld so the repository config decides", async () => {
  const [finding] = await run(echoer());
  assert.doesNotMatch(finding.path, /--over/);
});

test("--over is passed only when the user set a threshold", async () => {
  const [finding] = await run(echoer(), { threshold: 20 });
  assert.match(finding.path, /--over 20/);
});

test("--all is never passed, so the editor agrees with CI", async () => {
  const [finding] = await run(echoer());
  assert.doesNotMatch(finding.path, /--all/);
});

test("a breach exits non-zero and is still a successful scan", async () => {
  const binary = fakeCli(`
    process.stdin.resume();
    process.stdin.on("end", () => {
      console.log(JSON.stringify({ thresholds: {}, breaches: 1, findings: [
        { path: "a.php", line: 3, name: "f", score: 22, language: "php", domain: "root" },
      ] }));
      process.exit(1);
    });
  `);

  const findings = await run(binary);
  assert.equal(findings.length, 1);
  assert.equal(findings[0].score, 22);
});

test("unparseable output rejects with whatever the tool said", async () => {
  const binary = fakeCli(`
    process.stdin.resume();
    process.stdin.on("end", () => {
      process.stderr.write("the grammar exploded");
      process.exit(1);
    });
  `);

  await assert.rejects(run(binary), /the grammar exploded/);
});

test("output that is not a report is refused rather than trusted", async () => {
  const binary = fakeCli(`
    process.stdin.resume();
    process.stdin.on("end", () => { console.log(JSON.stringify({ unexpected: true })); });
  `);

  await assert.rejects(run(binary));
});

test("a tool that exits without reading its input rejects instead of crashing the host", async () => {
  const binary = fakeCli(`
    process.stderr.write("no language handles this extension");
    process.exit(1);
  `);

  await assert.rejects(run(binary, { text: "x".repeat(1 << 20) }), /no language handles/);
});
