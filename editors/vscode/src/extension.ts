import * as vscode from "vscode";
import { resolveBinary, type Binary } from "./binary";
import { scan, type Finding } from "./run";

const MISSING_BINARY_DISMISSED = "bonsai.missingBinaryDismissed";

/**
 * Coalesces keystrokes. TypeScript projects keep far more files open than PHP ones, and one
 * process per keystroke is felt immediately.
 */
const DEBOUNCE_MS = 300;

let diagnostics: vscode.DiagnosticCollection;
let binary: Binary | undefined;
const pending = new Map<string, NodeJS.Timeout>();

interface Settings {
  enable: boolean;
  path: string;
  languages: string[];
  threshold: number | undefined;
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  diagnostics = vscode.languages.createDiagnosticCollection("bonsai");
  context.subscriptions.push(diagnostics);

  binary = await resolveBinary(settings().path);
  if (binary === undefined) {
    await reportMissingBinary(context);
  }

  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument((document) => void refresh(document)),
    vscode.workspace.onDidSaveTextDocument((document) => void refresh(document)),
    vscode.workspace.onDidChangeTextDocument((event) => schedule(event.document)),
    vscode.workspace.onDidCloseTextDocument((document) => forget(document)),
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (event.affectsConfiguration("bonsai")) {
        binary = await resolveBinary(settings().path);
        await refreshAll();
      }
    }),
  );

  await refreshAll();
}

export function deactivate(): void {
  for (const timer of pending.values()) {
    clearTimeout(timer);
  }
  pending.clear();
  diagnostics?.dispose();
}

function settings(): Settings {
  const config = vscode.workspace.getConfiguration("bonsai");
  const threshold = config.get<number | null>("threshold", null);

  return {
    enable: config.get<boolean>("enable", true),
    path: config.get<string>("path", ""),
    languages: config.get<string[]>("languages", [
      "php",
      "javascript",
      "javascriptreact",
      "typescript",
      "typescriptreact",
    ]),
    threshold: typeof threshold === "number" ? threshold : undefined,
  };
}

function schedule(document: vscode.TextDocument): void {
  const key = document.uri.toString();
  const existing = pending.get(key);
  if (existing !== undefined) {
    clearTimeout(existing);
  }

  pending.set(
    key,
    setTimeout(() => {
      pending.delete(key);
      void refresh(document);
    }, DEBOUNCE_MS),
  );
}

function forget(document: vscode.TextDocument): void {
  const key = document.uri.toString();
  const existing = pending.get(key);
  if (existing !== undefined) {
    clearTimeout(existing);
    pending.delete(key);
  }
  diagnostics.delete(document.uri);
}

async function refreshAll(): Promise<void> {
  await Promise.all(vscode.workspace.textDocuments.map((document) => refresh(document)));
}

async function refresh(document: vscode.TextDocument): Promise<void> {
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
    const findings = await scan({
      binary,
      file: document.uri.fsPath,
      workspaceRoot: workspace.uri.fsPath,
      threshold,
      text: document.getText(),
    });

    diagnostics.set(document.uri, findings.map((finding) => toDiagnostic(document, finding)));
  } catch (error) {
    // A broken scan should not leave stale diagnostics implying the file is clean.
    diagnostics.delete(document.uri);
    console.error("bonsai:", error);
  }
}

function toDiagnostic(document: vscode.TextDocument, finding: Finding): vscode.Diagnostic {
  const line = Math.max(0, Math.min(finding.line - 1, document.lineCount - 1));
  const range = document.lineAt(line).range;

  const diagnostic = new vscode.Diagnostic(
    range,
    `${finding.name} has a cognitive complexity of ${finding.score}`,
    vscode.DiagnosticSeverity.Warning,
  );
  diagnostic.source = "bonsai";
  diagnostic.code = "cognitive-complexity";

  return diagnostic;
}

/**
 * Shown once per installation. Staying silent makes the extension look broken; warning on
 * every activation nags anyone who has not installed the tool yet.
 */
async function reportMissingBinary(context: vscode.ExtensionContext): Promise<void> {
  if (context.globalState.get<boolean>(MISSING_BINARY_DISMISSED) === true) {
    return;
  }

  const install = "Installation instructions";
  const dismiss = "Don't show again";

  const choice = await vscode.window.showWarningMessage(
    "bonsai was not found. Install it, or set `bonsai.path` to an existing binary.",
    install,
    dismiss,
  );

  if (choice === install) {
    await vscode.env.openExternal(
      vscode.Uri.parse("https://github.com/ryckakas/bonsai-lint#install"),
    );
  } else if (choice === dismiss) {
    await context.globalState.update(MISSING_BINARY_DISMISSED, true);
  }
}
