#!/usr/bin/env python3
"""Vendor the tree-sitter grammar crates with a wasm32 branch added to their build scripts.

The grammars' `build.rs` files compile C with `cc` and have no wasm32 handling, so on
`wasm32-unknown-unknown` — a freestanding target with no system libc headers — they fail at
`#include <stdlib.h>`. tree-sitter's own build script already solves this: it defines
`TREE_SITTER_WASM_STDLIB` and adds the header subset that `tree-sitter-language` ships and
exports via `DEP_TREE_SITTER_LANGUAGE_WASM_HEADERS`. That recipe simply was never propagated
to the grammar crates.

Both patches are upstreamable. Until they land, this reproduces them locally rather than
committing several megabytes of vendored parser C.

    python3 vendor-grammars.py
"""

from __future__ import annotations

import glob
import os
import pathlib
import shutil
import sys

HERE = pathlib.Path(__file__).parent
VENDOR = HERE / "vendor"

# The wasm32 branch, written to match each crate's own local variable name for the cc::Build.
PATCH = """
    // wasm32-unknown-unknown is freestanding: no system libc headers exist, so the grammar's
    // `#include <stdlib.h>` fails to compile. tree-sitter ships the header subset and
    // tree-sitter-language exports its path; the grammar crates just never used it.
    if std::env::var("TARGET")
        .unwrap_or_default()
        .starts_with("wasm32-unknown")
    {{
        let wasm_headers = std::env::var("DEP_TREE_SITTER_LANGUAGE_WASM_HEADERS")
            .expect("tree-sitter-language must export wasm headers for wasm32 targets");
        {config}
            .define("TREE_SITTER_WASM_STDLIB", "")
            .include(&wasm_headers);
    }}
"""

GRAMMARS = [
    {
        "crate": "tree-sitter-go",
        "version": "0.25",
        "build": "bindings/rust/build.rs",
        "anchor": '    c_config.std("c11").include(src_dir);\n',
        "config": "c_config",
    },
    {
        "crate": "tree-sitter-java",
        "version": "0.23",
        "build": "bindings/rust/build.rs",
        "anchor": '    c_config.std("c11").include(src_dir);\n',
        "config": "c_config",
    },
    {
        "crate": "tree-sitter-typescript",
        "version": "0.23",
        "build": "bindings/rust/build.rs",
        "anchor": '        .flag_if_supported("-Wno-unused-parameter");\n',
        "config": "config",
    },
    {
        "crate": "tree-sitter-html",
        "version": "0.23",
        "build": "bindings/rust/build.rs",
        "anchor": '        .flag_if_supported("-Wno-implicit-fallthrough");\n',
        "config": "c_config",
    },
    {
        "crate": "tree-sitter-php",
        "version": "0.24",
        "build": "bindings/rust/build.rs",
        "anchor": '    c_config.std("c11").include(&php_dir);\n',
        "config": "c_config",
    },
]


def registry_source(crate: str, version: str) -> pathlib.Path:
    pattern = os.path.expanduser(
        f"~/.cargo/registry/src/*/{crate}-{version}.*"
    )
    matches = sorted(p for p in glob.glob(pattern) if pathlib.Path(p).is_dir())
    if not matches:
        sys.exit(
            f"{crate} {version}.* is not in the cargo registry.\n"
            f"Run `cargo fetch` from the repository root first."
        )
    return pathlib.Path(matches[-1])


def vendor(grammar: dict) -> None:
    source = registry_source(grammar["crate"], grammar["version"])
    target = VENDOR / grammar["crate"]

    if target.exists():
        shutil.rmtree(target)
    shutil.copytree(source, target)

    # Registry checkouts are read-only and carry a checksum that a modified copy would fail.
    for path in target.rglob("*"):
        path.chmod(path.stat().st_mode | 0o200)
    (target / ".cargo-checksum.json").unlink(missing_ok=True)

    build = target / grammar["build"]
    text = build.read_text()

    if "TREE_SITTER_WASM_STDLIB" in text:
        print(f"  {grammar['crate']}: already patched")
        return

    anchor = grammar["anchor"]
    if anchor not in text:
        sys.exit(
            f"{grammar['crate']}: could not find the anchor line in {grammar['build']}.\n"
            f"The crate's build script changed; update the anchor in this script."
        )

    patch = PATCH.format(config=grammar["config"])
    build.write_text(text.replace(anchor, anchor + patch, 1))
    print(f"  {grammar['crate']}: patched ({source.name})")


def main() -> None:
    VENDOR.mkdir(exist_ok=True)
    print(f"vendoring grammars into {VENDOR.relative_to(HERE.parent.parent)}")
    for grammar in GRAMMARS:
        vendor(grammar)
    print("done. Now run ./build-wasm.sh")


if __name__ == "__main__":
    main()
