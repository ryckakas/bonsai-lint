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

- **Two units with one qualified name cannot both be baselined.** A baseline is
  `{path: {qualified_name: score}}`, so when a file declares the same name twice the second write
  wins and the other unit can never be accepted, leaving an unchanged re-run failing forever. The
  trigger is a name the namer cannot qualify: `export const Widget = defineComponent({ setup(){} })`
  yields a bare `setup`, because `bound_name` does not unwrap a lone *object* argument the way
  `is_sole_callable_argument` unwraps a lone callable, so two components in one file collide.
  `disambiguate` deliberately leaves declared duplicates alone, which is right for the report but
  leaves the baseline with an un-silenceable finding. Teaching `bound_name` to unwrap a sole
  object argument would give `Widget::setup` and fix both, at the cost of changing existing
  baseline keys. Go reaches the same collision through package-level tables: every row of
  `map[string]*cmd{"hg": {run: func…}, "git": {run: func…}}` binds a literal to `run`. Java
  overloads never reach it, because a method's key carries its parameter types.
- **`super.f()` counts as recursion in TypeScript.** In Java, `super.m()` inside `m()` is an
  override calling its parent and is not recursion. TypeScript still lists `super` among a
  method's self-receivers, so `method() { super.method(); }` costs +1 there. Aligning the two
  would change existing TypeScript scores.
- **Java through Maven and Gradle.** Java builds usually reach tools through Maven or Gradle, and
  often only through an internal Maven mirror, so a GitHub download is not an option there. The
  binary would travel as per-platform Maven Central artifacts, in the pattern `protoc` uses,
  resolved by a Maven and a Gradle plugin, and published by a dist job as the Go module is.
  SDKMAN! and an IntelliJ plugin would come after.
- **Newer Java syntax.** tree-sitter-java 0.23.5 has seen no grammar work since 2023, so a few
  Java 21–25 constructs parse with an error. The surrounding code still scores, and the list is
  in [docs/scoring-rules.md](docs/scoring-rules.md). A newer grammar release is picked up by
  bumping the dependency, and the grammar contract names anything it renamed.

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
- **Trusted publishing.** Both npm and crates.io now support OIDC from GitHub Actions, which
  would remove the stored tokens that have to be rotated when they expire.
- **A long lived editor server.** The extension currently starts a process per analysis. A
  `--server` mode reusing the `--stdin` input shape would cut the per keystroke cost.
- **Type aware linting for the extension.** The TypeScript source has no linter beyond `tsc`.
  Its async surface is where `no-floating-promises` would earn its place.
