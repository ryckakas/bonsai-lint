# Architecture & development

[← Back to README](../README.md)

```text
crates/
├── bonsai-core/        the scorer: parsed tree in, scores out. no I/O, no serde, no grammars
├── bonsai-lang-go/     Go node kinds, field names and hooks, and its generated-file check
├── bonsai-lang-java/   Java node kinds, field names and hooks, and its generated-file check
├── bonsai-lang-php/    PHP node kinds, field names and hooks
├── bonsai-lang-ts/     TypeScript and TSX, sharing one spec across both dialects
├── bonsai-lang-vue/    Vue SFCs: locates the script blocks, scores them with the TS spec
├── bonsai-engine/      registry, configuration, domains, baselines, the scan driver
├── bonsai-lint/        the CLI, producing the `bonsai-lint` binary
├── bonsai-testkit/     the grammar contract harness, used by every language crate
└── bonsai-wasm/        the playground's WebAssembly bindings, a separate workspace
```

`bonsai-core` depends on nothing but `tree-sitter`. It can be embedded without dragging in
serde, the filesystem or any grammar.

## The language seam

A language is described mostly by data. `LanguageSpec` is a `&'static` value giving the node
kinds for each role and the field names the walker should read. What a table cannot say, such as
how a unit is named, what a call resolves to or where an if-chain's parts are, it says through
`Hooks`, a trait. A second trait, `LanguageDescriptor`, says which files belong to the language
and where the code in them is. The two vary independently: Vue reads its code exactly as
TypeScript does but finds it inside a `.vue` file, and `.ts` and `.tsx` are one reading over two
grammars.

That spec is then *compiled* once per process against a `tree_sitter::Language`, resolving every
kind string to a `u16` id and every field name to a `FieldId`, into flat arrays. The walker
indexes an array by `node.kind_id()` rather than comparing strings, so adding languages costs
nothing at scan time.

Compilation is fallible, and that is the point: if a grammar upgrade renames a node, the spec
fails to compile with a message naming it, rather than silently scoring zero for that construct
forever.

### What a language decides, and what core decides

The hooks read the tree; core does the arithmetic. A language decides which receiver spellings
reach the unit being scored, which nodes make up an `if` chain, and which file names hold no
code. Core decides what each of those costs: +1, +nesting, a flat +1 per `else if`, one per run
of like operators. With the arithmetic in one place, scoring the same logic the same in every
language is a property of the design rather than something each crate has to get right.

A trait method is required only when every language must answer it, and has a default only when
that default is right for a language without the feature. A method added later ships with a
default that keeps today's behaviour, so adding one never edits an existing language. The example
on `Hooks` implements only the required methods, so a new required method fails
`cargo test -p bonsai-core --doc` and has to be argued for.

That is also the limit of the design. A new *kind* of variation still costs one change in core, a
new trait method, even though no other language sees it. Go needed two: how a method reaches
itself, `unit_scope`, and how an if-chain is read, `if_parts`. Java needed two more, both with
defaults: `unit_body`, because a `static { }` block leaves its body unfielded, and
`call_reaches_unit`, because overloads share a name. Its keys also carry parameter types,
through `UnitName::with_signature`: the key includes the signature, and recursion never matches
against it.

### Adding a language

1. Add a crate with the grammar dependency, a `LanguageSpec`, a type implementing `Hooks` and a
   descriptor implementing `LanguageDescriptor`.
2. Write a fixture that exercises every kind you declared, and wire up
   `GrammarFixture::assert_contract()`. It will tell you what you got wrong.
3. Pin the scores in `tests/spec.rs`: tables of snippets asserted against stated totals, with
   the same test names the other languages use, beside `naming.rs`, `suppression.rs` and
   `toplevel.rs`.
4. Register the descriptor in `bonsai-engine/src/registry.rs` behind a cargo feature, forwarded
   by `bonsai-lint` and on in both `default` lists.
5. Add the feature to the CI matrix and give it a `cli_<id>.rs` end-to-end suite.
   `cli_report.rs` lists every threshold key. The `--lang` help reads the registry, so the CLI
   needs no edit.
6. Outside the workspace: a `LANG_` constant and a vendored grammar patch in `bonsai-wasm`, and
   an activation event and `bonsai-lint.languages` entry in the extension.

Most of the work is the fixture and the score corpus, not the spec, and nothing in core or the
engine changes beyond the registry line and the features. A grammar whose shape the defaults
cannot read overrides that hook in its own crate: Go overrides `if_parts`, because its `else` has
no node of its own and its `if` takes an initializer beside the condition. Java's `else` has no
node either, so it overrides `if_parts` as well.

### Languages embedded in a host syntax

A `.vue` file is mostly not code. `LanguageDescriptor::extract` is an optional hook returning the
byte ranges that hold the scorable text and the grammar to read them with; `bonsai-lang-vue` uses
`tree-sitter-html` to find the `<script>` blocks and hands back the TypeScript or TSX grammar
according to `lang`.

The ranges go to `Parser::set_included_ranges` against the **whole file**, not an extracted
substring. Tree-sitter then reports every node at its position in the original file, so a line
number needs no offset and a baseline key cannot drift by the length of a template.

Each block is parsed on its own, because tree-sitter concatenates included ranges and a line
comment closing one block would otherwise run into the next and swallow it whole. The driver then
adds the blocks' `<toplevel>` findings together, since two of them would collide as one baseline
key.

A host grammar has no `LanguageSpec`, so nothing fails compilation when it renames a node. Two
things stand in for that: a contract test over the kinds the extractor reads, and a scan warning
when a file contains `<script` but yielded no block.

#### Why Vue is a language id when Vue is not a language

A `.vue` file contains TypeScript or JavaScript; Vue is a file format around it, and the scores
come from the TypeScript spec either way. It still gets its own id, deliberately.

The id is the unit of user-facing control. It is what `--lang`, `--over LANG=N`, a `[section]` in
the config and the JSON `language` field all take. Folding Vue into `typescript` would make a
component indistinguishable from a service, and the two have different complexity profiles: a
`[vue]` budget separate from `[typescript]` is the main reason anyone would want the distinction.
Being able to scan only components, or only exclude them, follows from the same choice.

The id lives on the `LanguageSpec` and `registry::language_ids` deduplicates by it, so a distinct
id requires a distinct spec. That is what `bonsai_lang_ts::spec` exists for: `VUE_SPEC` is the
TypeScript spec under another name, sharing every kind list and every hook, so the same logic
scores the same in a `.vue` block and a `.ts` file. Nothing about the scoring diverges; only the
label does.

The cost is honest and small: a seventh crate to version and publish, and a second kind-id table
compiled per grammar. The alternative, a `vue` feature inside `bonsai-lang-ts`, would put
`tree-sitter-html` in the dependency graph of every TypeScript build and introduce the first
`#[cfg]` inside a language crate, where gating otherwise lives only in `bonsai-engine`.

### How a call reaches its own unit

Recursion costs +1, and only a direct syntactic self-reference is detectable. What counts as one
differs by language, so `Hooks::unit_scope` answers it once per unit with a `UnitScope`: the
receiver spellings through which a call of the unit's name reaches the unit, and whether a bare
call does. PHP lists `$this`, `self` and `static`. TypeScript lists `this`, `super` and the last
segment of the enclosing container's path. Both let a bare call count.

A Go method sits at file scope, not inside its type, so its `UnitScope` also carries the path
segment a class body supplies elsewhere: `func (s *Stack[T]) Push()` is keyed `Stack::Push`. It
reaches itself through `s` or `(*s)`, or through its type in a method expression such as
`(*Stack[T]).Push(s)`, and never through a bare `Push()`, since Go cannot call a method without
its receiver.

Java lists `this`, the enclosing class's name and `Outer.this`, but not `super`: `super.m()` runs
the parent's implementation, not this one. Overloads share a name, so `call_reaches_unit` also
checks that a call's argument count fits the method's parameters, where varargs accepts any
number beyond the fixed ones. `process(o) { process(o, user()); }` is then a delegating overload
rather than recursion. An overload taking the same number of arguments still reads as a self-call,
since only types could tell it apart.

### Generated files

`LanguageDescriptor::is_generated` lets a language recognise machine-written files by their
contents. Go's convention is a `// Code generated … DO NOT EDIT.` line before the `package`
clause. The check needs the source, so it runs on the worker after the read, and the file is then
dropped uncounted, exactly as a `.min.js` that the planner never admits. `--stdin` and the wasm
build apply the same check, so an editor showing a generated file agrees with CI. Java has no
single convention, so its rule reads what the common generators write: a file whose comments
before the first line of code say both "generated" and "do not edit", as protobuf, Thrift, Avro
and JavaCC output does.

`LanguageDescriptor::unscored_suffixes` is the same idea decided by name alone, before the read:
the TypeScript descriptor lists `.d.ts`, `.d.mts` and `.d.cts`, and the TSX one the `.min.js`
family.

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
6. for every `if` and else-if node in the fixture, the language's own `if_parts` recognises every
   part of the chain. A grammar can reshape a chain without renaming anything, and an
   unrecognised part would otherwise be scored as plain code.

## Building

Needs Rust 1.90 or newer via [rustup](https://rustup.rs), a floor set by `tree-sitter-language`
rather than by this project, and one CI builds against on every pull request so the number stays
honest.

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace --all-features
cargo deny check
cargo shear
typos
taplo fmt --check
zizmor .github
```

CI runs exactly these on every pull request, plus the feature subsets below, a build on the
minimum supported Rust version, and the tests again on `x86_64-unknown-linux-musl`.
`cargo build --release` produces the binary users get. `cargo deny` applies `deny.toml` to the
dependency tree: no known advisory, a license from its list, one version of each crate and
crates.io as the only source. `cargo shear` fails on a dependency that nothing uses. `typos`
checks spelling, with the deliberate misspellings the tests feed in listed in `_typos.toml`, and
`taplo` checks TOML formatting against `.taplo.toml`. `zizmor` audits the workflows, which hold
the publishing tokens: every action pinned to a commit, least-privilege tokens, no template
injection. `.github/zizmor.yml` lists the findings that only dist can change in the `release.yml`
it generates, each pinned to its line and column, so a regenerated `release.yml` fails the job
until each finding in it is reviewed again. They, and `cargo hack` below, are separate tools:
`cargo install --locked cargo-deny cargo-shear cargo-hack typos-cli taplo-cli zizmor`.

Every language is behind a cargo feature, and the registry has to keep compiling with any
subset — including none. CI builds every combination with `cargo-hack`, which reads the features
from the manifest, so a new language needs no CI edit.

```bash
cargo hack build -p bonsai-lint --feature-powerset
cargo build -p bonsai-lint --no-default-features --features php      # one subset
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
  does not live inside the walk. There is one exception, and it is deliberate: if the machine
  will not grant a worker thread at all, the scan runs inline on the caller's thread rather than
  failing, and that thread is sized by whoever spawned it. Being refused a thread is a reason to
  be slow, not to abandon the run. The CLI is unaffected because `main` already runs everything
  on a thread it sized itself; an embedder calling `Scanner::scan` should do the same.
- How many workers run is capped twice over: by the number of files, since a two-file scan has
  nothing to spread across eight threads, and by four times `available_parallelism`, since past
  that the spawning costs more than the parallelism returns. Uncapped, `--jobs 5000` measures
  slower than `--jobs 1`.
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
`dist generate` rather than hand-editing it. The actions it uses are pinned to commits in
`[dist.github-action-commits]`. A dist upgrade can move to newer action tags, and the pins have
to follow: resolve the new tags to commits, update the table, and regenerate. Dependabot ignores
those four actions, since a bump it made to `release.yml` would fail `dist plan`.

**Linux ships twice per architecture.** The glibc build needs the builder's glibc, 2.35. The
static musl build runs on any Linux, so it serves Alpine and older glibc alike. dist's installer
script and npm wrapper choose between them from the host's libc, and the Go launcher does the same
from `ldd --version`. The musl build replaces musl's allocator with mimalloc, through its
`override` feature, because tree-sitter's C code calls `malloc` directly. musl's allocator
serialises threads: on the HERO corpus on arm64, a parallel scan took 16.3 s against glibc's
0.94 s, and 0.84 s with mimalloc. Nothing but the release and CI's musl job compiles that target.

**The version moves when compatibility does.** The library crates are published, and a CLI
release depends on them by a caret range that `cargo install` resolves without the lockfile. A
breaking 0.3.x of `bonsai-core` would therefore stop `cargo install bonsai-lint@0.3.0` from
compiling, whether or not anyone else embeds it. CI's required Public API job runs
`cargo-semver-checks` against the latest release on crates.io. A pull request that breaks the
API bumps the workspace to 0.4.0 in the same change, and after that further breaks pass until
0.4.0 ships. The job checks only library crates crates.io already has: a new language crate has
no baseline until its first publish. It fetches the newest cargo-semver-checks on each run,
because the tool reads rustdoc's JSON output, which changes with Rust releases.

**Every user-facing name is `bonsai-lint`** — the crate, the binary, `bonsai-lint.toml`,
`.bonsai-lint-baseline.json`, the `bonsai-lint-ignore` marker and the extension's
`bonsai-lint.*` settings. The bare `bonsai` namespace belongs to unrelated projects on
crates.io, npm and the VS Code Marketplace, so nothing here claims it. Naming the crate and the
binary alike also keeps `dist` honest: it names the artifacts, the Homebrew formula and the npm
package after the package, so the command users get matches the thing they installed.

**The VS Code extension versions independently** of the CLI, and the two numbers are not
expected to match. Publishing is manual — `vsce` needs an Azure DevOps PAT.

**The extension depends on the `bonsai-lint` npm package** on a caret range. Under 0.x a caret
stops at the minor version, so `^0.3.0` accepts 0.3.x but not 0.4.0, and a minor CLI release
needs the range raised along with the relock below. It resolves the binary from
`bonsai-lint.path` first, then from that package, then from `PATH`.

**Release ordering.** `npm ci` installs the exact version pinned in
`editors/vscode/package-lock.json`, and that can only name a version `dist` has already
published. So the CLI goes first — bump, tag, publish — and the extension is relocked and
repackaged afterwards:

```bash
cd editors/vscode && npm install
```

Pointing the dependency at an unpublished version turns CI red, since `npm ci` cannot resolve it.
Note also that the `.vsix` ships the npm *wrapper* rather than the executable, and that wrapper
is pinned at package time — so a CLI release does not reach existing extension users until the
extension is republished.
