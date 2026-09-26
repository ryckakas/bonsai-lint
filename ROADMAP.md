# Roadmap

[← Back to README](README.md)

Nothing here is promised for a particular release. Items are listed in the order they are
likely to be worth doing, and each one records the measurement that motivated it.

## Faster discovery

Scoring is parallel as of the `--jobs` work; discovery is not, and it is now the whole of what
is left. Measured against a 1.26 million line monorepo on a ten-core M5:

```text
--jobs 1    3.15s
--jobs 10   0.77s      4.1x
```

Restricting the same scan to one language at a time isolates the shared term: the walk costs
about **0.24s**, which was 8% of the serial run and is now roughly a third of the parallel one.

Two separate things sit inside that 0.24s, and neither has been measured on its own yet:

- **The tree is walked twice.** `Workspace::undeclared_config_warnings` calls
  `config_directories_below`, an unbounded `ignore` walk of everything under the scan roots,
  before the scan walk proper — purely to find `bonsai-lint.toml` files that are not declared
  domains. On a repository with no stray configs it walks the whole tree to report nothing.
  Collecting those directories during the scan walk that already happens would remove a full
  traversal. Measure the split first; the 0.24s above covers both walks together.
- **The scan walk itself is serial.** `ignore::WalkBuilder` exposes `build_parallel()`, but note
  that it cannot also host the scoring: its visitors are spawned with `std::thread::scope` and
  no stack-size control, and the scorer needs a 256 MB stack. So this is a parallel producer
  feeding the existing pool, not a merge of the two. The care is that the `seen` dedupe decides
  which spelling of a duplicated path is reported, and diagnostics are emitted in walk order —
  both become an explicit deterministic sort rather than a free consequence of walking in order.

## Smaller known items

- **Two units with one qualified name share one baseline entry.** A baseline is
  `{path: {qualified_name: score}}`, so when a file declares the same name twice the entry keeps
  the higher score. An unchanged re-run passes, but the lower-scoring unit can grow up to the
  other's score unnoticed. The trigger is a name the namer cannot qualify:
  `export const Widget = defineComponent({ setup(){} })` yields a bare `setup`, because
  `bound_name` does not unwrap a lone *object* argument the way `is_sole_callable_argument`
  unwraps a lone callable, so two components in one file collide.
  `disambiguate` deliberately leaves declared duplicates alone, which is right for the report but
  leaves the two sharing one budget in the baseline. Teaching `bound_name` to unwrap a sole
  object argument would give `Widget::setup` and fix both, at the cost of changing existing
  baseline keys. Go reaches the same collision through package-level tables: every row of
  `map[string]*cmd{"hg": {run: func…}, "git": {run: func…}}` binds a literal to `run`. Python
  reaches it through a def redefined under `if`/`else`, and through `functools.singledispatch`
  implementations that are all named `_`. Java overloads never reach it, because a method's key
  carries its parameter types, and neither do Python's property accessors, because the accessor
  joins the key.
- **`super.f()` counts as recursion in TypeScript.** In Java, `super.m()` inside `m()` is an
  override calling its parent and is not recursion. TypeScript still lists `super` among a
  method's self-receivers, so `method() { super.method(); }` costs +1 there. Aligning the two
  would change existing TypeScript scores.
- **Java through Maven and Gradle.** Java builds usually reach tools through Maven or Gradle, and
  often only through an internal Maven mirror, so a GitHub download is not an option there. The
  binary would travel as per-platform Maven Central artifacts, in the pattern `protoc` uses,
  resolved by a Maven and a Gradle plugin, and published by a dist job as the Go module is.
  SDKMAN! and an IntelliJ plugin would come after.
- **Python through PyPI and pre-commit.** Python teams install linters with pip, uv or pipx and
  run them from pre-commit, rarely from npm or Homebrew. The binary would travel as per-platform
  wheels on PyPI, with a `.pre-commit-hooks.yaml` beside it, published by a dist job as the Go
  module is.
- **Newer Java syntax.** tree-sitter-java 0.23.5 has seen no grammar work since 2023, so a few
  Java 21–25 constructs parse with an error. The surrounding code still scores, and the list is
  in [docs/scoring-rules.md](docs/scoring-rules.md). A newer grammar release is picked up by
  bumping the dependency, and the grammar contract names anything it renamed.
- **Python grammar gaps.** tree-sitter-python 0.25.0 parses a few constructs with an error: a
  slice in a parameter annotation, several starred items in one subscript, type parameter
  defaults, and a bracketed continuation line indented less than its block. On CPython's
  standard library and Django that is 3 of 4,782 files. Its next release also turns
  `expression_statement` into a hidden supertype. The Python crate reads that node only as an
  optional wrapper, and its naming and suppression tests fail if it ever comes to depend on it.
- **A unit's own default parameter values are not scored.** A unit scores its body, and the
  top-level pass skips a unit whole, so `def f(x=1 if flag else 2)` and
  `function f(x = a ? 1 : 2) {}` both lose their ternary. A nested function's defaults already
  count toward the enclosing unit, one level deeper, which charges them to the function's scope
  in both languages. Python evaluates a default when the `def` runs, in the enclosing scope, and
  TypeScript when the call does, inside the function. Scoring them would be one change across every
  language, deciding which of the two scopes each one is charged to, and it would raise existing
  scores.
- **The decorator-factory exemption.** Other implementations exempt a function that holds only a
  nested function and its `return` from the nesting a closure adds. bonsai-lint applies it in no
  language, so the same shape scores the same everywhere. Adopting it would have to be one change
  across every language, and it would lower existing scores.
- **Parallel CPU overhead.** A parallel scan spends 1.7–1.8× the CPU time of `--jobs 1` in every
  language: 6.9s against 4.0s on 1.5 million lines of Python on a ten-core M5. Some of that is
  efficiency cores doing less work per second; the rest may be contention worth profiling. On
  Python, tree-sitter parsing alone is about 70% of a serial scan.

- **A domain cannot opt a language out.** Per-language thresholds work per domain, but there is
  no way to say that a domain is TypeScript only. The workaround is an `exclude` glob.
  Analysed in [docs/features/domain-language-scope.md](docs/features/domain-language-scope.md).
- **Shipping the CLI inside the extension.** The `.vsix` carries no binary today. Bundling the
  npm wrapper costs 23 KB and makes it fetch its own on first use; one package per platform
  costs about 1.6 MB and fetches nothing. Analysed in
  [docs/features/extension-cli-bundling.md](docs/features/extension-cli-bundling.md).
- **A Windows on ARM build.** There is no native one: the npm wrapper and the Go launcher both run
  the x64 binary under emulation. dist can build `aarch64-pc-windows-msvc`, but the grammars are
  C, so that cross-compile needs proving first.
- **Checksum verification in the npm wrapper.** The npm package downloads the release archive
  without checking it, while the installer script, the Homebrew formula and the Go launcher all
  check a sha256 recorded before the download. dist already publishes one per archive.
- **Trusted publishing for npm.** crates.io already publishes this way (see `publish-crates.yml`),
  but npm still takes the stored `NPM_TOKEN`. dist's generated npm job has no `id-token: write`,
  and it runs Node 20, whose npm is older than the 11.5.1 that OIDC needs. Replacing dist's npm
  job with a custom publish job, as `publish-go.yml` does for Go, would remove the token.
- **A long lived editor server.** The extension currently starts a process per analysis. A
  `--server` mode reusing the `--stdin` input shape would cut the per keystroke cost.
- **Type aware linting for the extension.** The TypeScript source has no linter beyond `tsc`.
  Its async surface is where `no-floating-promises` would earn its place.
