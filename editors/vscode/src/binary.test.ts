import { strict as assert } from "node:assert";
import { chmodSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { resolveBinary } from "./binary";

test("an existing configured path wins", async () => {
  const directory = mkdtempSync(join(tmpdir(), "bonsai-lint-binary-"));
  const file = join(directory, "bonsai-lint");
  writeFileSync(file, "#!/bin/sh\nexit 0\n");
  chmodSync(file, 0o755);

  const binary = await resolveBinary(file);

  assert.equal(binary?.source, "setting");
  assert.equal(binary?.command, file);
  assert.deepEqual(binary?.prefixArgs, []);
});

test("a configured path that does not exist falls through", async () => {
  const binary = await resolveBinary("/nowhere/bonsai-lint");
  assert.notEqual(binary?.source, "setting");
});
