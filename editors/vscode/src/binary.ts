import { existsSync } from "node:fs";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const run = promisify(execFile);

/**
 * How to invoke bonsai. The npm package ships a Node wrapper rather than a bare
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
export async function resolveBinary(configured: string): Promise<Binary | undefined> {
  if (configured.trim() !== "" && existsSync(configured)) {
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

function resolveBundled(): string | undefined {
  try {
    return require.resolve("bonsai-lint/run-bonsai.js");
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
