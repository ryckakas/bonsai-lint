#!/usr/bin/env bash
# Builds the playground module for wasm32-unknown-unknown.
#
# Two things the default toolchain does not give you:
#   · the patched grammars (see vendor-grammars.py)
#   · a GNU-format archiver. macOS `ar` writes BSD archives, which wasm-ld cannot read; the
#     symptom is `undefined symbol: ts_node_child_by_field_name` at link time. rustup's
#     llvm-tools ships an llvm-ar that writes the right format.
set -euo pipefail

cd "$(dirname "$0")"

if [ ! -d vendor/tree-sitter-typescript ] || [ ! -d vendor/tree-sitter-go ] \
  || [ ! -d vendor/tree-sitter-java ] || [ ! -d vendor/tree-sitter-python ]; then
  echo "grammars not vendored yet; running vendor-grammars.py"
  python3 vendor-grammars.py
fi

if ! rustup target list --installed | grep -q wasm32-unknown-unknown; then
  rustup target add wasm32-unknown-unknown
fi

LLVM_AR="$(find "$(rustc --print sysroot)" -name llvm-ar -type f 2>/dev/null | head -1)"
if [ -z "$LLVM_AR" ]; then
  echo "llvm-ar not found. Run: rustup component add llvm-tools" >&2
  exit 1
fi

AR_wasm32_unknown_unknown="$LLVM_AR" \
  cargo build --release --target wasm32-unknown-unknown

OUT="target/wasm32-unknown-unknown/release/bonsai_wasm.wasm"
printf 'built %s (%s)\n' "$OUT" "$(du -h "$OUT" | cut -f1)"
