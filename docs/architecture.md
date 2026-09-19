# Architecture & development

[← Back to README](../README.md)

```text
crates/
├── bonsai-core/        the scorer: parsed tree in, scores out. no I/O, no serde, no grammars
├── bonsai-lang-php/    PHP node kinds, field names and hooks
├── bonsai-lang-ts/     TypeScript and TSX, sharing one spec across both dialects
├── bonsai-engine/      registry, configuration, domains, baselines, the scan driver
├── bonsai-lint/        the CLI, producing the `bonsai` binary
└── bonsai-testkit/     the grammar contract harness, used by every language crate
```

`bonsai-core` depends on nothing but `tree-sitter`. It can be embedded without dragging in
serde, the filesystem or any grammar.

## The language seam

A language is described by data, not code. `LanguageSpec` is a `&'static` value giving the node
kinds for each role, the field names the walker should read, and a handful of function pointers
for the parts that genuinely differ between languages.

That spec is then *compiled* once per process against a `tree_sitter::Language`, resolving every
kind string to a `u16` id and every field name to a `FieldId`, into flat arrays. The walker
indexes an array by `node.kind_id()` rather than comparing strings, so adding languages costs
nothing at scan time.

Compilation is fallible, and that is the point: if a grammar upgrade renames a node, the spec
fails to compile with a message naming it, rather than silently scoring zero for that construct
forever.

### Adding a language

1. Add a crate with the grammar dependency and a `LanguageSpec`.
2. Write a fixture that exercises every kind you declared, and wire up
   `GrammarFixture::assert_contract()`. It will tell you what you got wrong.
3. Add a golden-score corpus under `tests/fixtures/`.
4. Register the descriptor in `bonsai-engine/src/registry.rs` behind a cargo feature.

Most of the work is the fixture and the score corpus, not the spec.

### The grammar contract

`GrammarFixture::assert_contract()` makes six assertions, because an id-indexed table has
failure modes a string match does not:

1. the fixture parses cleanly;
2. the spec compiles against the grammar;
3. every declared field name resolves — a renamed `consequence` would silently zero every `if`;
4. every declared kind is actually produced by the fixture, not merely present in the symbol
   table;
5. `id_for_node_kind` agrees with `kind_id` for every declared kind, so tree-sitter aliasing
   cannot make the whole table miss;
6. every child under an `if`'s alternative field is an else or else-if kind, which a grammar can
   break without renaming anything.

## Building

Needs Rust 1.90 or newer via [rustup](https://rustup.rs), a floor set by `tree-sitter-language`
rather than by this project, and one CI builds against on every pull request so the number stays
honest.

```bash
cargo build --release
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Every language is behind a cargo feature, and the registry has to keep compiling with any
subset — including none. CI builds all four combinations.

```bash
cargo build -p bonsai-lint --no-default-features --features php
```

## Releasing

Releasing is [`dist`](https://github.com/axodotdev/cargo-dist): pushing a `v*` tag builds every
target, generates the installers and publishes a GitHub Release. Preview with `dist plan`.
`.github/workflows/release.yml` is generated — edit `dist-workspace.toml` and re-run
`dist generate` rather than hand-editing it.

The CLI crate is named `bonsai-lint` deliberately: `dist` names the artifacts, the Homebrew
formula and the npm package after the package, while the binary it installs is `bonsai`.

**The VS Code extension versions independently** of the CLI, and the two numbers are not
expected to match. Publishing is manual — `vsce` needs an Azure DevOps PAT.

**Bootstrap ordering.** The extension is meant to depend on the `bonsai-lint` npm package on a
caret range, so it picks up CLI releases without a bump of its own. That dependency cannot exist
until `dist` has published the CLI for the first time, so it is deliberately absent from
`editors/vscode/package.json` right now — otherwise `npm ci` fails and CI is red before the
first release. After the first tag:

```bash
cd editors/vscode && npm install --save bonsai-lint@^0.1.0
```

Until then the extension resolves the binary from `bonsai.path` or from `PATH`, which is the
same fallback chain it always uses.
