"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const node_assert_1 = require("node:assert");
const node_fs_1 = require("node:fs");
const node_os_1 = require("node:os");
const node_path_1 = require("node:path");
const node_test_1 = require("node:test");
const run_1 = require("./run");
const scripts = (0, node_fs_1.mkdtempSync)((0, node_path_1.join)((0, node_os_1.tmpdir)(), "bonsai-fake-cli-"));
let counter = 0;
/**
 * A stand-in for the CLI. It has to be a script file rather than `node -e`, because node reads
 * the trailing `--format json` as its own option and refuses to start.
 */
function fakeCli(script) {
    const file = (0, node_path_1.join)(scripts, `fake-${(counter += 1)}.js`);
    (0, node_fs_1.writeFileSync)(file, script);
    return { command: process.execPath, prefixArgs: [file], source: "path" };
}
/** Echoes the arguments it was given and the text it was piped, as one finding. */
const echoer = () => fakeCli(`
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
function run(binary, overrides = {}) {
    return (0, run_1.scan)({
        binary,
        file: "any.php",
        workspaceRoot: process.cwd(),
        text: "",
        ...overrides,
    });
}
(0, node_test_1.test)("the buffer is piped in rather than read from disk", async () => {
    const [finding] = await run(echoer(), { text: "<?php // unsaved" });
    node_assert_1.strict.equal(finding.name, "<?php // unsaved");
});
(0, node_test_1.test)("the file is passed as --stdin-path so the language can be chosen", async () => {
    const [finding] = await run(echoer(), { file: "/repo/src/Thing.php" });
    node_assert_1.strict.match(finding.path, /--stdin --stdin-path \/repo\/src\/Thing\.php/);
});
(0, node_test_1.test)("--over is withheld so the repository config decides", async () => {
    const [finding] = await run(echoer());
    node_assert_1.strict.doesNotMatch(finding.path, /--over/);
});
(0, node_test_1.test)("--over is passed only when the user set a threshold", async () => {
    const [finding] = await run(echoer(), { threshold: 20 });
    node_assert_1.strict.match(finding.path, /--over 20/);
});
(0, node_test_1.test)("--all is never passed, so the editor agrees with CI", async () => {
    const [finding] = await run(echoer());
    node_assert_1.strict.doesNotMatch(finding.path, /--all/);
});
(0, node_test_1.test)("a breach exits non-zero and is still a successful scan", async () => {
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
    node_assert_1.strict.equal(findings.length, 1);
    node_assert_1.strict.equal(findings[0].score, 22);
});
(0, node_test_1.test)("unparseable output rejects with whatever the tool said", async () => {
    const binary = fakeCli(`
    process.stdin.resume();
    process.stdin.on("end", () => {
      process.stderr.write("the grammar exploded");
      process.exit(1);
    });
  `);
    await node_assert_1.strict.rejects(run(binary), /the grammar exploded/);
});
(0, node_test_1.test)("output that is not a report is refused rather than trusted", async () => {
    const binary = fakeCli(`
    process.stdin.resume();
    process.stdin.on("end", () => { console.log(JSON.stringify({ unexpected: true })); });
  `);
    await node_assert_1.strict.rejects(run(binary));
});
//# sourceMappingURL=run.test.js.map