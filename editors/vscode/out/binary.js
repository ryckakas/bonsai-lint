"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.resolveBinary = resolveBinary;
const node_fs_1 = require("node:fs");
const node_child_process_1 = require("node:child_process");
const node_util_1 = require("node:util");
const run = (0, node_util_1.promisify)(node_child_process_1.execFile);
/**
 * Explicit setting first, then the package this extension depends on, then whatever is on
 * PATH — so someone who installed via Homebrew keeps their own version, and someone who
 * installed nothing still gets a working extension.
 */
async function resolveBinary(configured) {
    if (configured.trim() !== "" && (0, node_fs_1.existsSync)(configured)) {
        return { command: configured, prefixArgs: [], source: "setting" };
    }
    const bundled = resolveBundled();
    if (bundled !== undefined) {
        return { command: process.execPath, prefixArgs: [bundled], source: "bundled" };
    }
    if (await isOnPath("bonsai")) {
        return { command: "bonsai", prefixArgs: [], source: "path" };
    }
    return undefined;
}
function resolveBundled() {
    try {
        return require.resolve("bonsai-lint/run-bonsai.js");
    }
    catch {
        return undefined;
    }
}
async function isOnPath(command) {
    try {
        await run(command, ["--version"]);
        return true;
    }
    catch {
        return false;
    }
}
//# sourceMappingURL=binary.js.map