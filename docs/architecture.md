# Architecture & development

[← Back to README](../README.md)

```text
crates/
├── bonsai-core/        the scorer: parsed tree in, scores out. no I/O, no serde, no grammars
├── bonsai-lang-php/    PHP node kinds, field names and hooks
├── bonsai-lang-ts/     TypeScript and TSX, sharing one spec across both dialects
├── bonsai-engine/      registry, configuration, domains, baselines, the scan driver
├── bonsai-lint/        the CLI, producing the `bonsai-lint` binary
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
2. the spec compiles against the grammar, which resolves every kind and every field name — a
   renamed `consequence` would silently zero every `if`;
3. every declared kind is actually produced by the fixture, not merely present in the symbol
   table;
4. where a grammar renamed a kind between releases and both spellings are declared, one of them
   is live;
5. `id_for_node_kind` agrees with `kind_id` for every declared kind, so tree-sitter aliasing
   cannot make the whole table miss;
6. every child under an `if`'s alternative field is an else or else-if kind, which a grammar can
   break without renaming anything.

## Building

Needs Rust 1.90 or newer via [rustup](https://rustup.rs), a floor set by `tree-sitter-language`
rather than by this project, and one CI builds against on every pull request so the number stays
honest.

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

CI runs exactly these on every pull request, plus the feature subsets below and a build on the
minimum supported Rust version. `cargo build --release` produces the binary users get.

Every language is behind a cargo feature, and the registry has to keep compiling with any
subset — including none. CI builds all four combinations.

```bash
cargo build -p bonsai-lint --no-default-features --features php
```

Feature subsets exist so that adding or removing a language stays a clean operation and the
`#[cfg]` seams keep being exercised. **They are not a shipping option.** `dist` publishes one
binary with every language built in; there is no slim artifact and no variant for a user to
choose.

## The scan driver

A few decisions in `bonsai-engine` and the CLI are invisible from the outside and easy to undo
by accident:

- Every path compared against a domain root is canonical, so `/tmp` and `/private/tmp` never
  end up on opposite sides of a `starts_with`. Scan roots pay one `canonicalize` each; files
  under them are joined arithmetically.
- Globs stop `*` at `/`, as `.gitignore` does. `packages/*` names direct children, and the
  domain walk is bounded to that depth, which is what keeps an editor's per-keystroke scan cheap.
- Every thread that parses reserves a 256 MB stack. The walkers are recursive, and generated
  code nests far deeper than a default thread allows. `ignore`'s own `build_parallel()` spawns
  its visitors with `std::thread::scope` and no way to set a stack size, which is why scoring
  does not live inside the walk.
- The walk is planned on one thread, the files are scored on many, and the results are replayed
  in plan order, with walk errors keeping their slot in that list beside the files.
- Determinism rests on two separate mechanisms, and it is worth knowing which does what. The
  **report body** is ordered by `rank`, a stable sort over score, path and line; because the path
  is unique per file, completion order cannot disturb it, and the only possible ties are within
  one file, which survive because a file's findings are merged as one contiguous batch. The
  **diagnostics** are the opposite: warnings and errors accumulate in merge order alone, so
  breaking the plan-order replay leaves the report looking correct while stderr silently
  reorders. That is why the tests guarding it use several invalid-UTF-8 files rather than one —
  a single warning cannot be emitted out of order, and an earlier version of these tests missed
  exactly that bug for exactly that reason.
- A file that is not valid UTF-8 is decoded leniently with a warning: the replaced bytes sit in
  strings and comments, which do not score. An unreadable file is an error and fails the run.
- A closed stdout — `bonsai-lint --all . | head` — ends the output quietly and leaves the exit
  code to the findings.

## Releasing

Releasing is [`dist`](https://github.com/axodotdev/cargo-dist): pushing a `v*` tag builds every
target, generates the installers and publishes a GitHub Release. Preview with `dist plan`.
`.github/workflows/release.yml` is generated — edit `dist-workspace.toml` and re-run
`dist generate` rather than hand-editing it.

**Every user-facing name is `bonsai-lint`** — the crate, the binary, `bonsai-lint.toml`,
`.bonsai-lint-baseline.json`, the `bonsai-lint-ignore` marker and the extension's
`bonsai-lint.*` settings. The bare `bonsai` namespace belongs to unrelated projects on
crates.io, npm and the VS Code Marketplace, so nothing here claims it. Naming the crate and the
binary alike also keeps `dist` honest: it names the artifacts, the Homebrew formula and the npm
package after the package, so the command users get matches the thing they installed.

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

Until then the extension resolves the binary from `bonsai-lint.path`, then from that package once
it exists, then from `PATH` — the same fallback chain it always uses.
