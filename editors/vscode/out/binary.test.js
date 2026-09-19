"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const node_assert_1 = require("node:assert");
const node_fs_1 = require("node:fs");
const node_os_1 = require("node:os");
const node_path_1 = require("node:path");
const node_test_1 = require("node:test");
const binary_1 = require("./binary");
(0, node_test_1.test)("an existing configured path wins", async () => {
    const directory = (0, node_fs_1.mkdtempSync)((0, node_path_1.join)((0, node_os_1.tmpdir)(), "bonsai-lint-binary-"));
    const file = (0, node_path_1.join)(directory, "bonsai-lint");
    (0, node_fs_1.writeFileSync)(file, "#!/bin/sh\nexit 0\n");
    (0, node_fs_1.chmodSync)(file, 0o755);
    const binary = await (0, binary_1.resolveBinary)(file);
    node_assert_1.strict.equal(binary?.source, "setting");
    node_assert_1.strict.equal(binary?.command, file);
    node_assert_1.strict.deepEqual(binary?.prefixArgs, []);
});
(0, node_test_1.test)("a configured path that does not exist falls through", async () => {
    const binary = await (0, binary_1.resolveBinary)("/nowhere/bonsai-lint");
    node_assert_1.strict.notEqual(binary?.source, "setting");
});
//# sourceMappingURL=binary.test.js.map