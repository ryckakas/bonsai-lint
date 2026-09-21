# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`bonsai-lint` is a cognitive complexity linter written in Rust that reads PHP, JavaScript,
TypeScript and Vue single-file components via tree-sitter grammars, without executing any of it.
One static binary, no PHP or Node runtime required to run the analysis. It ships as a Cargo
workspace plus an independent VS Code extension.

## Commands

```bash
cargo fmt --all -- --check                          # formatting (rustfmt.toml: 100 cols)
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features                            # whole workspace
cargo test -p bonsai-lang-php                         # one crate
cargo test -p bonsai-lang-php --test grammar          # one integration test file
cargo build --release                                 # produces target/release/bonsai-lint
```

CI (`.github/workflows/ci.yml`) runs exactly the fmt/clippy/test commands above, plus a build
matrix across Linux/macOS/Windows, a feature-combination build (`php`, `ts`, `php,ts`, `vue`,
`ts,vue`, `php,ts,vue`, none — each
`cargo build -p bonsai-lint --no-default-features --features "<set>"`), and a
build-only check on the MSRV read from `Cargo.toml` (`rust-version`). Match these locally before
pushing rather than relying on CI to catch it.

Every language is behind a cargo feature; `vue` implies `ts` because it reuses that spec. The
registry must keep compiling with any feature subset, including none — this is exercised, not
incidental, so don't add code that assumes a language is always present without a `#[cfg]` guard
matching the existing pattern.

VS Code extension (`editors/vscode/`, own `package.json`, independent version number):

```bash
cd editors/vscode
npm ci
npx tsc -p . --noEmit      # typecheck
npm test                    # compiles then runs node --test on out/*.test.js
npx --yes @vscode/vsce package --out /tmp/extension.vsix
```

## Architecture

```
crates/
├── bonsai-core/        the scorer: parsed tree in, scores out. No I/O, no serde, no grammars.
├── bonsai-lang-php/    PHP node kinds, field names and hooks
├── bonsai-lang-ts/     TypeScript and TSX, sharing one spec across both dialects
├── bonsai-lang-vue/    Vue SFCs: locates the script blocks, scores them with the TS spec
├── bonsai-engine/      registry, configuration, domains, baselines, the scan driver
├── bonsai-lint/        the CLI, producing the `bonsai-lint` binary
└── bonsai-testkit/     the grammar contract harness, used by every language crate
```

`bonsai-core` depends on nothing but `tree-sitter`, so it can be embedded without pulling in
serde, the filesystem, or any grammar crate.

**The language seam is data, not code.** A `LanguageSpec` is a `&'static` value naming the node
kinds for each syntactic role, the field names the walker reads, and a few function pointers for
the parts that genuinely differ between languages. That spec is compiled once per process
against a `tree_sitter::Language`, resolving every kind string to a `u16` id and every field name
to a `FieldId` into flat arrays — the walker indexes by `node.kind_id()` rather than comparing
strings, so adding languages doesn't cost anything at scan time. Compilation is fallible on
purpose: a grammar upgrade that renames a node fails spec compilation with a message naming it,
rather than silently scoring that construct as zero forever.

**Adding a language** means: a new crate with the grammar dependency and a `LanguageSpec`; a
fixture exercising every declared kind, wired through `GrammarFixture::assert_contract()`; a
golden-score corpus under `tests/fixtures/`; and registering the descriptor in
`bonsai-engine/src/registry.rs` behind a cargo feature. Most of the effort is the fixture and
score corpus, not the spec itself.

**`GrammarFixture::assert_contract()`** (`bonsai-testkit`) makes six assertions per language,
because an id-indexed table fails in ways a string match doesn't: the fixture parses cleanly; the
spec compiles against the grammar (resolves every kind/field); every declared kind is actually
produced by the fixture, not merely present in the grammar's symbol table; where a grammar
renamed a kind across releases and both spellings are declared, one is live; `id_for_node_kind`
agrees with `kind_id` for every declared kind (tree-sitter aliasing can otherwise make the whole
table miss silently); and every child under an `if`'s alternative field is an else/else-if kind.
When touching a `LanguageSpec` or bumping a `tree-sitter-*` grammar version, run this contract
first — it is designed to tell you exactly what broke.

**Scan driver invariants** (`bonsai-engine` + the CLI) — easy to break by accident, so preserve
them when touching scan/domain/path code:
- Every path compared against a domain root is canonicalized first, so `/tmp` vs `/private/tmp`
  never land on opposite sides of a `starts_with`.
- Globs stop `*` at `/` and let `**` cross it, matching `.gitignore` semantics; the domain walk
  depth is bounded by this, which keeps a per-keystroke editor scan cheap.
- Every thread that parses reserves a 256 MB stack (`bonsai_engine::STACK_SIZE`) because the
  walkers are recursive and generated code nests deeper than a default stack allows. `ignore`'s
  `build_parallel()` cannot host the scoring: it spawns visitors with `std::thread::scope` and
  no stack-size control. The one exception is a machine that grants no worker thread at all:
  the scan then runs inline on the caller's thread, which the engine did not size. The CLI is
  safe because `main` already wraps the run in a sized thread; an embedder must do the same.
- Worker count is capped by the file count and by four times `available_parallelism`. Past that
  the spawns cost more than the parallelism returns — uncapped, `--jobs 5000` measures slower
  than `--jobs 1`.
- The walk is planned on one thread, files are scored on many, results are replayed in plan
  order, walk errors keeping their slot beside the files.
- Two mechanisms make that deterministic and they cover different things. The report body is
  ordered by `rank` (a stable sort over score/path/line, path unique per file), and its only
  ties — two findings on one line — survive because each file's findings merge as one contiguous
  batch. Warnings and errors have no such sort: they accumulate in merge order alone. So a broken
  replay shows up on stderr, not in the report, and a test for it needs several invalid-UTF-8
  files, not one.
- Invalid UTF-8 is decoded leniently with a warning (replacement bytes land in strings/comments,
  which don't score); an unreadable file is a hard error and fails the run.
- A closed stdout ends output quietly and leaves the exit code to the findings, not to the write
  error.

**Naming convention:** every user-facing name is `bonsai-lint` — crate, binary, `bonsai-lint.toml`,
`.bonsai-lint-baseline.json`, the `bonsai-lint-ignore` marker, the extension's `bonsai-lint.*`
settings. The bare `bonsai` namespace deliberately isn't claimed anywhere (it belongs to unrelated
projects on crates.io/npm/VS Code Marketplace).

## Testing conventions

- `bonsai-lang-php` and `bonsai-lang-ts` each have `tests/grammar.rs` (the contract, via
  `bonsai-testkit`), plus `naming.rs`, `spec.rs`, `suppression.rs`, `toplevel.rs`, and for TS,
  `tsx.rs`. Fixtures live in `tests/fixtures/`, shared helpers in `tests/common/mod.rs`.
- `bonsai-lang-vue` reuses the TypeScript spec under a second id, so it has no naming or scoring
  suite of its own. It has `grammar.rs` (the spec against both TS grammars), `host_grammar.rs`
  (the HTML grammar's kinds, which nothing else would fail on), `sfc.rs` (block extraction) and
  `vue.rs` (scoring, and that a reported line is a line in the `.vue` file).
- `bonsai-engine` integration tests (`baseline.rs`, `config.rs`, `scan.rs`) exercise config
  discovery, domains, and baseline read/write against real temp directories.
- `bonsai-lint/tests/` drives the compiled binary end-to-end, split by area (`cli_report.rs`,
  `cli_stdin.rs`, `cli_baseline.rs`, `cli_domains.rs`, `cli_parallel.rs`, `cli_vue.rs`) over a
  shared `tests/common/mod.rs`. Each file is its own test binary, so `--test cli_vue` runs in
  a fifth of a second while `cli_parallel` is the slow one.
- Cross-language parity (same logic in PHP and TypeScript scoring identically) is a tested
  property, not an assumption — see the README's "same code scores the same" example when
  changing shared scoring logic in `bonsai-core`.

## Comments

Don't comment. The exception is a short "why" — rationale, a trade-off, the reason a non-obvious
choice was made — and only where it is genuinely needed. Three lines is the ceiling; one is
usually right.

Never restate what the code already says. If a comment could be deleted without losing something
a reader could not recover from the code itself, delete it. Where a rule is correct *by omission*
— a value deliberately absent from a match arm or a list — pair the "why" with a named test
asserting the absence, since a comment alone cannot fail.

## Workspace-wide lint config (`Cargo.toml`)

`unsafe_code = "forbid"`, clippy `pedantic` warn, and `excessive_nesting`/`too_many_lines` are
**deny**, not warn (nesting threshold is 3, set in `clippy.toml`). This is a deliberate
dogfooding stance — the tool measures complexity/nesting in the languages it lints, and enforces
a version of the same discipline on its own Rust via clippy. Don't loosen these to get code to
compile; restructure instead.

## Releasing

Releases go through [`cargo-dist`](https://github.com/axodotdev/cargo-dist): pushing a `v*` tag
builds every target and publishes a GitHub Release, npm package, and Homebrew formula.
`.github/workflows/release.yml` is generated from `dist-workspace.toml` — edit the latter and run
`dist generate`, don't hand-edit the workflow. `dist plan` previews a release. The VS Code
extension version is independent of the CLI's and is published manually via `vsce` (needs an
Azure DevOps PAT).

`CHANGELOG.md` is not decoration: `dist` reads the section whose heading matches the tag and
publishes it as that release's notes, so a missing or misnamed section ships an empty release
page. Add the section before tagging, and keep the heading as `## [x.y.z] - YYYY-MM-DD`.

A release bumps `version` in the root `Cargo.toml` **and** the six internal path dependencies
beside it, which must match or cargo refuses to build. The npm package and Homebrew formula take
their version from that one field; neither is edited by hand. The extension is bumped afterwards,
because its lockfile can only pin a CLI version that is already published.

## Further reading

- [docs/architecture.md](docs/architecture.md) — the source for most of the above, in more depth
- [docs/scoring-rules.md](docs/scoring-rules.md) — the full cognitive-complexity increment table
- [ROADMAP.md](ROADMAP.md) — planned work and the measurements motivating it (e.g. discovery is
  still single-threaded, and walks the tree twice)
