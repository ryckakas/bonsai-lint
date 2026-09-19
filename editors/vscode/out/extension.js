"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = __importStar(require("vscode"));
const binary_1 = require("./binary");
const run_1 = require("./run");
const span_1 = require("./span");
const MISSING_BINARY_DISMISSED = "bonsai.missingBinaryDismissed";
/**
 * Coalesces keystrokes. TypeScript projects keep far more files open than PHP ones, and one
 * process per keystroke is felt immediately.
 */
const DEBOUNCE_MS = 300;
let diagnostics;
let binary;
const pending = new Map();
async function activate(context) {
    diagnostics = vscode.languages.createDiagnosticCollection("bonsai");
    context.subscriptions.push(diagnostics);
    binary = await (0, binary_1.resolveBinary)(settings().path);
    if (binary === undefined) {
        await reportMissingBinary(context);
    }
    context.subscriptions.push(vscode.workspace.onDidOpenTextDocument((document) => void refresh(document)), vscode.workspace.onDidSaveTextDocument((document) => void refresh(document)), vscode.workspace.onDidChangeTextDocument((event) => schedule(event.document)), vscode.workspace.onDidCloseTextDocument((document) => forget(document)), vscode.workspace.onDidChangeConfiguration(async (event) => {
        if (event.affectsConfiguration("bonsai")) {
            binary = await (0, binary_1.resolveBinary)(settings().path);
            await refreshAll();
        }
    }));
    await refreshAll();
}
function deactivate() {
    for (const timer of pending.values()) {
        clearTimeout(timer);
    }
    pending.clear();
    diagnostics?.dispose();
}
function settings() {
    const config = vscode.workspace.getConfiguration("bonsai");
    const threshold = config.get("threshold", null);
    return {
        enable: config.get("enable", true),
        path: config.get("path", ""),
        languages: config.get("languages", [
            "php",
            "javascript",
            "javascriptreact",
            "typescript",
            "typescriptreact",
        ]),
        threshold: typeof threshold === "number" ? threshold : undefined,
    };
}
function schedule(document) {
    const key = document.uri.toString();
    const existing = pending.get(key);
    if (existing !== undefined) {
        clearTimeout(existing);
    }
    pending.set(key, setTimeout(() => {
        pending.delete(key);
        void refresh(document);
    }, DEBOUNCE_MS));
}
function forget(document) {
    const key = document.uri.toString();
    const existing = pending.get(key);
    if (existing !== undefined) {
        clearTimeout(existing);
        pending.delete(key);
    }
    diagnostics.delete(document.uri);
}
async function refreshAll() {
    await Promise.all(vscode.workspace.textDocuments.map((document) => refresh(document)));
}
async function refresh(document) {
    const { enable, languages, threshold } = settings();
    if (!enable || !languages.includes(document.languageId) || document.uri.scheme !== "file") {
        diagnostics.delete(document.uri);
        return;
    }
    const workspace = vscode.workspace.getWorkspaceFolder(document.uri);
    if (workspace === undefined || binary === undefined) {
        return;
    }
    try {
        const findings = await (0, run_1.scan)({
            binary,
            file: document.uri.fsPath,
            workspaceRoot: workspace.uri.fsPath,
            threshold,
            text: document.getText(),
        });
        diagnostics.set(document.uri, findings.map((finding) => toDiagnostic(document, finding)));
    }
    catch (error) {
        // A broken scan should not leave stale diagnostics implying the file is clean.
        diagnostics.delete(document.uri);
        console.error("bonsai:", error);
    }
}
function toDiagnostic(document, finding) {
    const lines = document.getText().split(/\r?\n/);
    const span = (0, span_1.diagnosticSpan)(lines, finding.line - 1);
    const range = new vscode.Range(span.line, span.start, span.line, span.end);
    const diagnostic = new vscode.Diagnostic(range, `${finding.name} has a cognitive complexity of ${finding.score}`, vscode.DiagnosticSeverity.Warning);
    diagnostic.source = "bonsai";
    diagnostic.code = "cognitive-complexity";
    return diagnostic;
}
/**
 * Shown once per installation. Staying silent makes the extension look broken; warning on
 * every activation nags anyone who has not installed the tool yet.
 */
async function reportMissingBinary(context) {
    if (context.globalState.get(MISSING_BINARY_DISMISSED) === true) {
        return;
    }
    const install = "Installation instructions";
    const dismiss = "Don't show again";
    const choice = await vscode.window.showWarningMessage("bonsai was not found. Install it, or set `bonsai.path` to an existing binary.", install, dismiss);
    if (choice === install) {
        await vscode.env.openExternal(vscode.Uri.parse("https://github.com/ryckakas/bonsai-lint#install"));
    }
    else if (choice === dismiss) {
        await context.globalState.update(MISSING_BINARY_DISMISSED, true);
    }
}
//# sourceMappingURL=extension.js.map