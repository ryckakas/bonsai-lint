import * as vscode from "vscode";
import { resolveBinary, type Binary } from "./binary";
import { scan, type Finding } from "./run";
import { diagnosticSpan } from "./span";

const MISSING_BINARY_DISMISSED = "bonsai-lint.missingBinaryDismissed";

/**
 * Coalesces keystrokes. TypeScript projects keep far more files open than PHP ones, and one
 * process per keystroke is felt immediately.
 */
const DEBOUNCE_MS = 300;

let diagnostics: vscode.DiagnosticCollection;
let output: vscode.OutputChannel;
let binary: Binary | undefined;
const pending = new Map<string, NodeJS.Timeout>();
const reported = new Set<string>();

interface Settings {
  enable: boolean;
  path: string;
  languages: string[];
  threshold: number | undefined;
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  diagnostics = vscode.languages.createDiagnosticCollection("bonsai-lint");
  output = vscode.window.createOutputChannel("bonsai-lint");
  context.subscriptions.push(diagnostics, output);

  binary = await locateBinary();
  if (binary === undefined) {
    await reportMissingBinary(context);
  }

  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument((document) => void refresh(document)),
    vscode.workspace.onDidSaveTextDocument((document) => void refresh(document)),
    vscode.workspace.onDidChangeTextDocument((event) => schedule(event.document)),
    vscode.workspace.onDidCloseTextDocument((document) => forget(document)),
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (event.affectsConfiguration("bonsai-lint")) {
        binary = await locateBinary();
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
  const config = vscode.workspace.getConfiguration("bonsai-lint");
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

async function locateBinary(): Promise<Binary | undefined> {
  const { path } = settings();
  const found = await resolveBinary(path, vscode.workspace.workspaceFolders?.[0]?.uri.fsPath);
  if (path.trim() !== "" && found?.source !== "setting") {
    report(
      `bonsai-lint.path is set to "${path}" but nothing exists there; ` +
        `using ${found === undefined ? "nothing" : `the ${found.source} binary`} instead.`,
    );
  }
  return found;
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

  const version = document.version;
  const text = document.getText();

  try {
    const findings = await scan({
      binary,
      file: document.uri.fsPath,
      workspaceRoot: workspace.uri.fsPath,
      threshold,
      text,
    });

    // A save and a debounced edit can overlap; the slower, older result must not win.
    if (document.isClosed || document.version !== version) {
      return;
    }

    const lines = text.split(/\r?\n/);
    diagnostics.set(document.uri, findings.map((finding) => toDiagnostic(lines, finding)));
  } catch (error) {
    if (document.isClosed || document.version !== version) {
      return;
    }
    // A broken scan should not leave stale diagnostics implying the file is clean.
    diagnostics.delete(document.uri);
    report(error instanceof Error ? error.message : String(error));
  }
}

function toDiagnostic(lines: string[], finding: Finding): vscode.Diagnostic {
  const span = diagnosticSpan(lines, finding.line - 1);
  const range = new vscode.Range(span.line, span.start, span.line, span.end);

  const diagnostic = new vscode.Diagnostic(
    range,
    `${finding.name} has a cognitive complexity of ${finding.score}`,
    vscode.DiagnosticSeverity.Warning,
  );
  diagnostic.source = "bonsai-lint";
  diagnostic.code = "cognitive-complexity";

  return diagnostic;
}

/**
 * Every problem lands in the output channel; the popup fires once per distinct message, or a
 * broken `bonsai-lint.toml` would nag on every keystroke.
 */
function report(message: string): void {
  output.appendLine(message);
  if (reported.has(message)) {
    return;
  }
  reported.add(message);

  const show = "Show output";
  void vscode.window.showWarningMessage(`bonsai-lint: ${message}`, show).then((choice) => {
    if (choice === show) {
      output.show(true);
    }
  });
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
    "bonsai-lint was not found. Install it, or set `bonsai-lint.path` to an existing binary.",
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
