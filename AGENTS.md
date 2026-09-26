# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`bonsai-lint` is a multi-language cognitive complexity linter written in Rust. It reads source
through tree-sitter grammars without executing any of it; the README lists the supported
languages. One static binary, no language runtime or toolchain required to run the analysis. It
ships as a Cargo workspace plus an independent VS Code extension.

## Commands

```bash
cargo fmt --all -- --check                          # formatting (rustfmt.toml: 100 cols)
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features                            # whole workspace
cargo test -p bonsai-lang-php                         # one crate
cargo test -p bonsai-lang-php --test grammar          # one integration test file
cargo build --release                                 # produces target/release/bonsai-lint
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace --all-features
cargo deny check                                      # advisories, licenses, duplicates: deny.toml
cargo shear                                           # dependencies nothing uses
cargo hack build -p bonsai-lint --feature-powerset    # every language subset, including none
typos                                                 # spelling; deliberate misspellings: _typos.toml
taplo fmt --check                                     # TOML formatting: .taplo.toml
zizmor .github                                        # workflow security: .github/zizmor.yml
```

The last six are separate tools:
`cargo install --locked cargo-deny cargo-shear cargo-hack typos-cli taplo-cli zizmor`.

Every action in the workflows is pinned to a commit, with its version in a comment, and every
workflow but `release.yml` defaults to a read-only token. A new `uses:` line follows the same
form, and `zizmor` fails CI until it does.

CI (`.github/workflows/ci.yml`) runs exactly the commands above, plus a test matrix across
Linux/macOS/Windows, a build-only check on the MSRV read from `Cargo.toml` (`rust-version`), and
the tests on `x86_64-unknown-linux-musl`, the only build that swaps in mimalloc (it needs
`musl-tools`, so on macOS run it in an `ubuntu` container). Match these locally before pushing
rather than relying on CI to catch it.

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
├── bonsai-lang-go/     Go node kinds, field names and hooks, and its generated-file check
├── bonsai-lang-java/   Java node kinds, field names and hooks, and its generated-file check
├── bonsai-lang-php/    PHP node kinds, field names and hooks
├── bonsai-lang-python/ Python node kinds, field names and hooks, and its generated-file check
├── bonsai-lang-ts/     TypeScript and TSX, sharing one spec across both dialects
├── bonsai-lang-vue/    Vue SFCs: locates the script blocks, scores them with the TS spec
├── bonsai-engine/      registry, configuration, domains, baselines, the scan driver
├── bonsai-lint/        the CLI, producing the `bonsai-lint` binary
├── bonsai-testkit/     the grammar contract harness, used by every language crate
└── bonsai-wasm/        the playground's WebAssembly bindings, a separate workspace
```

`bonsai-core` depends on nothing but `tree-sitter`, so it can be embedded without pulling in
serde, the filesystem, or any grammar crate.

**The language seam is data plus two traits.** A `LanguageSpec` is a `&'static` value naming the
node kinds for each syntactic role and the field names the walker reads, plus
`&'static dyn Hooks` for the tree-reading that genuinely differs between languages (unit names,
callees, `unit_scope` for recursion, `if_parts` for if-chains). `LanguageDescriptor` is the
per-file-type trait (extensions, `extract`, `is_generated`, `unscored_suffixes`). Hooks read the
tree; the scoring arithmetic stays in core, so parity between languages is structural. A trait
method is required only when every language must answer it, and one added later ships with a
default that keeps today's behaviour; the `Hooks` doc example implements only the required
methods, so `cargo test -p bonsai-core --doc` fails if that rule is broken. That spec is
compiled once per process against a `tree_sitter::Language`, resolving every kind string to a
`u16` id and every field name to a `FieldId` into flat arrays — the walker indexes by
`node.kind_id()` rather than comparing strings, so adding languages doesn't cost anything at
scan time. Compilation is fallible on purpose: a grammar upgrade that renames a node fails spec
compilation with a message naming it, rather than silently scoring that construct as zero
forever.

**A language id is not a claim about languages.** Vue is a file format whose script blocks are
TypeScript, and `VUE_SPEC` is the TypeScript spec under another name, so the scores are identical.
It carries its own id because the id is what `--lang`, `--over LANG=N`, a `[section]` and the JSON
`language` field all key on, and a component deserves a budget separate from a service. Since
`registry::language_ids` deduplicates by `spec.id`, a distinct id is what forces a distinct spec.
The reasoning is in [docs/architecture.md](docs/architecture.md); don't collapse the id without
reading it.

**Adding a language** means: a new crate with the grammar dependency, a `LanguageSpec`, and types
implementing `Hooks` and `LanguageDescriptor`; a fixture exercising every declared kind, wired
through `GrammarFixture::assert_contract()`; the scores pinned as `(snippet, total)` tables in
`tests/spec.rs`; and registering the descriptor in `bonsai-engine/src/registry.rs` behind a cargo
feature. The full checklist (`cli_<id>.rs`, wasm, extension) is in
[docs/architecture.md](docs/architecture.md); CI's feature powerset reads the manifest and the
CLI's `--lang` help reads the registry, so neither needs an edit. Most of the effort is the
fixture and score tables, not the spec itself. A grammar shape the defaults can't read is handled
by overriding that hook in the language's own crate, as Go overrides `if_parts` for its node-less
`else` and its `if` initializer. Java overrides `if_parts` for the same missing `else` node,
`unit_body` because `static { }` leaves its block unfielded, and `call_reaches_unit` because
overloads share a name. Python overrides `if_parts` because `elif` holds its branch in
`consequence`, `unit_body` so a body of only `...` is not a unit, and `is_control_header` because
`a if c else b` leaves its condition unfielded. Core charges an `else` that no if chain claims
+1, which only Python's loop and `try` else reach.

**`GrammarFixture::assert_contract()`** (`bonsai-testkit`) makes six assertions per language,
because an id-indexed table fails in ways a string match doesn't: the fixture parses cleanly; the
spec compiles against the grammar (resolves every kind/field); every declared kind is actually
produced by the fixture, not merely present in the grammar's symbol table; where a grammar
renamed a kind across releases and both spellings are declared, one is live; `id_for_node_kind`
agrees with `kind_id` for every declared kind (tree-sitter aliasing can otherwise make the whole
table miss silently); and for every `if`/else-if node, the language's own `if_parts` recognises
every part of the chain (no `IfPart::Unrecognised`).
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
- A file its language marks as generated (`LanguageDescriptor::is_generated`: Go's
  `// Code generated … DO NOT EDIT.` header, or a Java or Python header comment saying both
  "generated" and "do not edit") is read, then dropped on the worker without being counted,
  like a `.min.js` the plan never admits; `--stdin` answers it with an empty report.
- Invalid UTF-8 is decoded leniently with a warning (replacement bytes land in strings/comments,
  which don't score); an unreadable file is a hard error and fails the run.
- A closed stdout ends output quietly and leaves the exit code to the findings, not to the write
  error.

**Naming the language set:** taglines and descriptions call the tool *multi-language* and never
enumerate languages. That covers the GitHub About text, the crate description (which npm and
Homebrew reuse), the extension's `displayName` and `description`, and the cover's alt text.

The supported languages are enumerated in exactly two places, the README intro and its
supported-languages table. Both are ordered by usage rather than by when support landed:
JavaScript, TypeScript, Vue, Python, Java, PHP, Go. That is the Stack Overflow developer survey
order, ranking a family by its most-used member: Vue sits beside TypeScript because it is scored
as TypeScript, and Python follows that family. A new language is slotted in by the same
ranking, and `docs/scoring-rules.md` orders its per-language tables and sections the same way.

Everything else that names languages is functional and follows the registry:
- the cargo features, which CI's feature powerset reads;
- the extension's activation events and its default `bonsai-lint.languages`;
- the search tags: GitHub topics and the extension's `keywords`.

**Naming convention:** every user-facing name is `bonsai-lint` — crate, binary, `bonsai-lint.toml`,
`.bonsai-lint-baseline.json`, the `bonsai-lint-ignore` marker, the extension's `bonsai-lint.*`
settings. The bare `bonsai` namespace deliberately isn't claimed anywhere (it belongs to unrelated
projects on crates.io/npm/VS Code Marketplace).

## Testing conventions

- `bonsai-lang-php` and `bonsai-lang-ts` each have `tests/grammar.rs` (the contract, via
  `bonsai-testkit`), plus `naming.rs`, `spec.rs`, `suppression.rs`, `toplevel.rs`, and for TS,
  `tsx.rs`. Fixtures live in `tests/fixtures/`, shared helpers in `tests/common/mod.rs`.
- `bonsai-lang-go` has the same five suites plus `generated.rs` (the generated-file header rule).
  Its `common::findings` also rejects a snippet that does not parse cleanly, since Go syntax is
  easy to get subtly wrong inside a string.
- `bonsai-lang-java` has the same suites as Go, `generated.rs` included, and the same
  parse-clean check. Its second helper, `findings_in_a_grammar_gap`, insists the snippet does
  *not* parse cleanly. It pins the workaround for syntax tree-sitter-java cannot read yet, such
  as `String @Nullable ... args`, and fails once a grammar upgrade learns it.
- `bonsai-lang-python` has the same suites as Go, `generated.rs` included, and the same
  parse-clean check, since indentation is easy to get wrong inside a Rust string. Its
  `common::score` wraps a flush-left snippet in `def target():` and indents every line, so the
  score tables stay readable.
- `bonsai-lang-vue` reuses the TypeScript spec under a second id, so it has no naming or scoring
  suite of its own. It has `grammar.rs` (the spec against both TS grammars), `host_grammar.rs`
  (the HTML grammar's kinds, which nothing else would fail on), `sfc.rs` (block extraction) and
  `vue.rs` (scoring, and that a reported line is a line in the `.vue` file).
- `bonsai-engine` integration tests (`baseline.rs`, `config.rs`, `scan.rs`) exercise config
  discovery, domains, and baseline read/write against real temp directories.
- `bonsai-lint/tests/` drives the compiled binary end-to-end, split by area (`cli_report.rs`,
  `cli_stdin.rs`, `cli_baseline.rs`, `cli_domains.rs`, `cli_parallel.rs`, `cli_vue.rs`,
  `cli_go.rs`, `cli_java.rs`, `cli_python.rs`) over a shared `tests/common/mod.rs`. Each file is
  its own test binary, so `--test cli_vue` runs in a fifth of a second while `cli_parallel` is
  the slow one.
- Cross-language parity (the same logic scoring identically in every language) is a tested
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

A set of allow-by-default `rustc` lints, clippy's `cargo` group, and hand-picked clippy
`restriction` and `nursery` lints are warn too. Each was clean when it was added, so a hit is new
code to fix, not a lint to switch off. Neither `restriction` nor `nursery` is a group to enable
wholesale: `restriction`'s lints contradict each other, and `nursery`'s are unsettled. An
`#[allow]` needs a `reason = "…"`, the same "why" a comment would give.

## Releasing

Releases go through [`cargo-dist`](https://github.com/axodotdev/cargo-dist): pushing a `v*` tag
builds every target and publishes a GitHub Release, npm package, Homebrew formula and Go module.
`.github/workflows/release.yml` is generated from `dist-workspace.toml` — edit the latter and run
`dist generate`, don't hand-edit the workflow. `dist plan` previews a release. The VS Code
extension version is independent of the CLI's and is published manually via `vsce` (needs an
Azure DevOps PAT).

Upgrading dist takes two more steps:
- **Re-pin its actions.** They are pinned in `[dist.github-action-commits]`, so point them at the
  commits behind the tags the new version uses.
- **Review `.github/zizmor.yml` again.** Its exceptions name `release.yml`'s findings by line and
  column, so the Workflow security job fails until they match the regenerated file.
  `zizmor --no-config .github/workflows/release.yml` lists the findings with their new positions.

`CHANGELOG.md` is not decoration: `dist` reads the section whose heading matches the tag and
publishes it as that release's notes, so a missing or misnamed section ships an empty release
page. Add the section before tagging, and keep the heading as `## [x.y.z] - YYYY-MM-DD`.

The version is `version` in the root `Cargo.toml` **and** the internal path dependencies beside
it, which must match or cargo refuses to build. The npm package, Homebrew formula and Go module
take their version from that one field; none is edited by hand. The extension is bumped
afterwards, because its lockfile can only pin a CLI version that is already published.

**The version moves when compatibility does.** A pull request that breaks a library crate's
public API bumps the workspace to the next breaking version (0.3.x → 0.4.0) in the same change.
CI's required **Public API** job (`cargo-semver-checks` against the latest release on crates.io)
fails until it does. A release that follows only compatible changes bumps the patch version in
the release PR. A crate that has never been published is left out until its first release.

crates.io is published separately, by dispatching `.github/workflows/publish-crates.yml` by hand
after the release, because a published version can never be taken back:
- **Normally it uses trusted publishing.** GitHub vouches for the run, and crates.io issues a token
  that lasts 30 minutes, then revokes it when the job ends. Each crate's crates.io settings name
  that workflow as a trusted publisher (owner `ryckakas`, repository `bonsai-lint`, workflow
  `publish-crates.yml`).
- **A release that adds a new crate, such as a language, sets `first_publish`.** crates.io trusts a
  workflow only for a crate that already exists there, so that one run uses the
  `CARGO_REGISTRY_TOKEN` secret. Create a short-lived token for it, add the new crate's trusted
  publisher once it is up, then delete the secret.
- **Run with `dry_run` first.** It packages and verifies every crate, uploads nothing, and needs no
  token.

The Go module `bonsai.kauneckas.dev/bonsai-lint` is a launcher that lives in
[bonsai-lint-go](https://github.com/ryckakas/bonsai-lint-go). `.github/workflows/publish-go.yml`
is a custom dist publish job, and it runs once the GitHub Release exists:
- **What it does:** it writes that repository's `release.go` from the released `dist-manifest.json`,
  tests the launcher, and pushes the tag `vX.Y.Z`. The release commit hangs off `main` and only
  the tag reaches the remote, so `main` keeps its placeholder. `main` ships as it stands at that
  moment, so land launcher changes there only when they are ready.
- **What it needs:** a `GO_MODULE_TOKEN` secret with contents write access to bonsai-lint-go,
  whose `main` takes pull requests only while its tags must stay unprotected for the job to push,
  and the page behind `https://bonsai.kauneckas.dev/bonsai-lint?go-get=1`, served from
  bonsai-lint-site. The Go proxy resolves the import path through that page, so it must be live
  before tagging.
- **A published Go version is permanent.** A failed publish is rerun with `workflow_dispatch`,
  `rehearsal` unticked. That is a no-op when the tag already exists with the same `release.go`,
  and it fails when the file differs, because a tag is never moved. A broken version is retracted
  from the next patch release's `go.mod`.
- **Rehearse a launcher change** with `workflow_dispatch` and `rehearsal` ticked. It runs every
  step, but pushes to bonsai-lint-go's `rehearsal` branch instead of tagging, and never contacts
  the proxy.

## Further reading

- [docs/architecture.md](docs/architecture.md) — the source for most of the above, in more depth
- [docs/scoring-rules.md](docs/scoring-rules.md) — the full cognitive-complexity increment table
- [ROADMAP.md](ROADMAP.md) — planned work and the measurements motivating it (e.g. discovery is
  still single-threaded, and walks the tree twice)
