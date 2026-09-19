import { existsSync } from "node:fs";
import { execFile } from "node:child_process";
import { isAbsolute, resolve } from "node:path";
import { promisify } from "node:util";

const run = promisify(execFile);

/**
 * How to invoke bonsai-lint. The npm package ships a Node wrapper rather than a bare
 * executable, so a resolved command may need `node` in front of it.
 */
export interface Binary {
  command: string;
  prefixArgs: string[];
  source: "setting" | "bundled" | "path";
}

/**
 * Explicit setting first, then the package this extension depends on, then whatever is on
 * PATH — so someone who installed via Homebrew keeps their own version, and someone who
 * installed nothing still gets a working extension.
 */
export async function resolveBinary(
  configured: string,
  workspaceRoot?: string,
): Promise<Binary | undefined> {
  const setting = configured.trim();
  if (setting !== "") {
    // A relative setting means relative to the project, not to wherever the editor started.
    const command =
      isAbsolute(setting) || workspaceRoot === undefined ? setting : resolve(workspaceRoot, setting);
    if (existsSync(command)) {
      return { command, prefixArgs: [], source: "setting" };
    }
  }

  const bundled = resolveBundled();
  if (bundled !== undefined) {
    return { command: process.execPath, prefixArgs: [bundled], source: "bundled" };
  }

  if (await isOnPath("bonsai-lint")) {
    return { command: "bonsai-lint", prefixArgs: [], source: "path" };
  }

  return undefined;
}

function resolveBundled(): string | undefined {
  try {
    return require.resolve("bonsai-lint/run-bonsai-lint.js");
  } catch {
    return undefined;
  }
}

async function isOnPath(command: string): Promise<boolean> {
  try {
    await run(command, ["--version"]);
    return true;
  } catch {
    return false;
  }
}
