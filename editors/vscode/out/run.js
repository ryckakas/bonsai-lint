"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.scan = scan;
const node_child_process_1 = require("node:child_process");
/**
 * The buffer is piped in rather than read from disk, so an unsaved edit is analysed as typed.
 *
 * `--over` is passed only when the user explicitly set a threshold. Passing it unconditionally
 * would silently override the repository's `bonsai.toml`, and the editor would then disagree
 * with CI — which is the one thing this integration must never do.
 *
 * `--all` is deliberately not passed: the editor should show exactly what CI would fail on,
 * baseline and suppressions included.
 */
function scan(options) {
    const { binary, file, workspaceRoot, threshold, text } = options;
    const args = [...binary.prefixArgs, "--format", "json", "--stdin", "--stdin-path", file];
    if (threshold !== undefined) {
        args.push("--over", String(threshold));
    }
    return new Promise((resolve, reject) => {
        const child = (0, node_child_process_1.execFile)(binary.command, args, { cwd: workspaceRoot }, (error, stdout, stderr) => {
            // A breach exits non-zero, which is the tool working, not failing. Only unparseable
            // output means something actually went wrong.
            const report = parseReport(stdout);
            if (report !== undefined) {
                resolve(report.findings);
                return;
            }
            reject(new Error(stderr.trim() !== "" ? stderr.trim() : (error?.message ?? "no output")));
        });
        child.stdin?.end(text);
    });
}
function parseReport(stdout) {
    if (stdout.trim() === "") {
        return undefined;
    }
    try {
        const parsed = JSON.parse(stdout);
        return isReport(parsed) ? parsed : undefined;
    }
    catch {
        return undefined;
    }
}
function isReport(value) {
    return (typeof value === "object" && value !== null && Array.isArray(value.findings));
}
//# sourceMappingURL=run.js.map