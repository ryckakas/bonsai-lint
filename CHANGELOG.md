# Changelog

[← Back to README](README.md)

Notable changes per release, in the format of [Keep a Changelog](https://keepachangelog.com).
This project follows [Semantic Versioning](https://semver.org). While it is pre-1.0, the minor
version is the compatibility unit: a backwards-compatible change is a patch bump (0.3.0 → 0.3.1),
and a breaking change to a library crate's public API moves to the next minor (0.3.x → 0.4.0).

`dist` reads the section matching a tag and uses it as that release's notes, so an entry here is
what a user reads on the GitHub release page.

## [Unreleased]

### Added

- **`--strict-baseline`, and `strict-baseline = true` in `bonsai-lint.toml`.** A baseline entry
  looser than the code fails the run with exit 1, so the baseline tightens as the code improves
  instead of accepting the old score for good. The key can be set per domain, and a domain
  without it takes the root's. Only a scan of a whole domain judges its baseline, so a pre-commit
  hook or an editor never fails on one.
- **`--format github`,** for GitHub Actions. Each breach becomes an annotation on its line of the
  pull request, a loose baseline entry one on the baseline file, and config warnings and read
  errors one on the run. It fails exactly when the text report does. Paths are made relative to
  `GITHUB_WORKSPACE`, so the step can run from any directory.

### Changed

- **Baseline entries the code has outgrown are named, one line each.** Besides entries that match
  nothing, which used to be only counted, a scan of the whole domain now reports an entry whose
  unit scores less than recorded, no longer scores over its threshold, or is suppressed with a
  reason: `root: src/a.php: busy is baselined at 28 but scores 21`. Without the new flag the exit
  code is unchanged, and the baseline format too, so existing baselines keep working. To silence
  the new lines, or before turning on strict mode, rewrite a baseline with `--write-baseline` over
  the whole workspace and the flags CI runs with.
- **The JSON report gains `loose_entries`,** always present and empty when there are none: the same
  entries, each with its `domain`, `path`, `unit`, `recorded` score, `reason`, current `score`,
  `threshold` and whether it is `strict`.
- **Each finding in the JSON report gains `end_line`,** the unit's last line.
- **For embedders of `bonsai-core` and `bonsai-engine`**, and breaking, which is why this is 0.5.0
  rather than 0.4.4:
  - `Baseline::loose` returns each loose entry as a `LooseEntry` with a `Looseness` reason, and
    replaces `Baseline::unmatched`.
  - `ConfigFile` and `Domain` gain `strict_baseline`, and `Report` gains `loose_entries`, built from
    the new `ReportedLooseEntry` and `LooseReason`.
  - `Finding` and `ReportedFinding` gain `end_line`, and `Baseline::recorded` returns the score a
    baseline accepts for a unit.

## [0.4.3] - 2026-09-27

### Added

- **Installing through Composer.** `composer require --dev bonsai-lint/bonsai-lint` adds it to a
  PHP project, and `vendor/bin/bonsai-lint` runs it. The package is a launcher with no
  dependencies that needs PHP 7.4 or newer. The first run of each version downloads that release's
  binary, checks it against a checksum recorded in the tagged package, and caches it inside
  `vendor/`. It honours the proxy variables the way Composer does, and never sends a GitHub or
  Composer token. It lives beside the Go launcher in
  [bonsai-lint-launcher](https://github.com/ryckakas/bonsai-lint-launcher), the renamed
  bonsai-lint-go, and one tag publishes both.

## [0.4.2] - 2026-09-27

### Added

- **Installing through PyPI.** `pip install bonsai-lint`, `uv tool install bonsai-lint` and
  `pipx install bonsai-lint` install the CLI, and `uvx bonsai-lint` runs it without installing.
  Each wheel is the release's own binary, byte for byte, with no Python code around it: macOS on
  Apple silicon and Intel, Linux on x86_64 and arm64 through the static build (glibc 2.17 or
  newer, or musl), and Windows on x64. There is no source distribution, so other platforms,
  including an ARM64 Python on Windows, get pip's "no matching distribution". The wheels are
  published at every release under the same version, through PyPI trusted publishing with
  attestations.
- **A pre-commit hook.** `- repo: https://github.com/ryckakas/bonsai-lint` with `id: bonsai-lint`
  installs the wheel and scores the staged files. It judges each file as `bonsai-lint` run from
  the repository root does, and a commit touching only files it skips, such as generated code,
  passes.
- **`--allow-no-files`.** A run whose paths hold no file it can score exits 0 instead of failing
  with "no supported files found". A missing path still fails the run.

## [0.4.1] - 2026-09-26

### Added

- **Python.** `.py` and `.pyw` files are scanned under a new `python` language id, so
  `--lang python`, `--over python=N` and a `[python]` section in the config all work. `.pyi`
  stubs are not scanned.
  - **Keys.** A method is keyed by its class, `OrderService::process`. A property's getter,
    setter and deleter share a name, so the accessor joins the key: `Cart::total`,
    `Cart::total.setter`, `Cart::total.deleter`, each baselined apart. A def whose body is only
    `...`, such as an `@overload` stub or a protocol method, is not a unit.
  - **Scoring.** `elif` scores as `else if`. An `else:` on a `for`, `while` or `try` costs +1, like
    any `else`. `match` costs one increment and its cases nest. Each `except` costs one, while
    `try`, `finally` and `with` are free. A conditional expression costs like a ternary, and its
    condition does not nest. A comprehension is free but raises nesting like a closure, as the
    callback chain it replaces does in JavaScript.
  - **Recursion.** A module function reaches itself by its bare name, and a method through
    `self`, `cls` or its class's full path, `Outer.Inner` for a nested class. A bare call inside
    a method names a global, and `super()` runs the parent's implementation, so neither is
    recursion.
  - **Units.** Lambdas, nested defs, comprehensions and the methods of local classes roll up into
    the enclosing function. Module code, class bodies and decorator arguments are the file's
    `<toplevel>`.
  - Built behind a `python` cargo feature, on by default.

  A file whose comments before the first line of code say both "generated" and "do not edit" is
  skipped, neither scored nor counted, on disk and through `--stdin`. That covers protobuf, gRPC
  and Thrift output. A Django migration says only "Generated by Django", so it stays scored.

  Upgrading a repository that contains `.py` files will report findings that were previously
  invisible. Run `bonsai-lint --write-baseline .` to adopt them.

  There is no PyPI package or pre-commit hook yet. On a Python project, install through Homebrew,
  the installer script or `npx bonsai-lint`.

### Changed

- **For embedders of `bonsai-core`:**
  - An `else` that no if chain claims now costs +1 on its own. Only Python's loop and `try` else
    produce one, so no existing language's scores change.
  - `Hooks::is_control_header` is a new provided method, defaulting to `false`. It names a header
    child that has no field, such as the condition in the middle of Python's `a if c else b`.

### Fixed

- When two units in one file still share a key, such as a Python def redefined under `if`/`else`,
  `--write-baseline` records the higher of their scores, so an unchanged rerun passes. It kept the
  last one reported, the lower score, and the higher one failed every run after. The two now share
  one budget, so the lower one can grow up to the higher score unnoticed.
- Two Java anonymous classes passed to one constructor, as in
  `new Dispatcher(new Runnable() { … }, new Runnable() { … })`, no longer share a key. Both
  methods were `C::run()`, so the baseline kept one score and an unchanged rerun failed on the
  other. A callback passed to a constructor now takes the constructor's type and its argument
  position, as a method call's arguments already did: `C::new Dispatcher#0::run()` and
  `C::new Dispatcher#1::run()`. Lambdas in the same place move from `<anonymous>` to the same
  form, so a baseline holding either needs `--write-baseline` once.

## [0.4.0] - 2026-09-25

### Added

- **Java.** `.java` files are scanned under a new `java` language id, so `--lang java`,
  `--over java=N` and a `[java]` section in the config all work.
  - **Keys.** Java overloads freely, so a method or constructor is keyed by its parameter types
    as well as its name: `OrderService::process(Order, User)`. Each type is its simple name,
    without type arguments or annotations. Two overloads are therefore baselined apart, and
    switching between an import and a qualified name never re-keys one.
  - **Recursion.** A call to the method's own name counts only when its argument count fits, so
    a delegating overload, `process(o) { process(o, user()); }`, is not recursion. Neither is
    `super.process()`, which runs the parent's implementation.
  - **Scoring.** A `switch` costs one increment in statement and expression form, a `when` guard
    costs only its operators, and each `catch` costs one. `synchronized`, `&`, `|` and `^` are
    free.
  - **Units.** Lambdas and the methods of anonymous and local classes roll up into the enclosing
    method. A `static { }` block is a unit of its own, `C::<static>`. Field initialisers and
    instance initializer blocks are the file's `<toplevel>`.
  - Built behind a `java` cargo feature, on by default.

  A file whose comments before the first line of code say both "generated" and "do not edit" is
  skipped, neither scored nor counted, on disk and through `--stdin`. That is how protobuf,
  Thrift, Avro and JavaCC output marks itself.

  Upgrading a repository that contains `.java` files will report findings that were previously
  invisible. Run `bonsai-lint --write-baseline .` to adopt them.

  There is no Maven or Gradle plugin yet. On a Java project, install through Homebrew, the
  installer script or `npx bonsai-lint`.

- **musl Linux builds.** Static `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`
  archives ship beside the glibc ones, so Alpine and other musl systems get a prebuilt binary.
  They need no libc at all, so they also serve hosts whose glibc is older than 2.35, such as
  Debian 11, Amazon Linux 2 and RHEL 8. The installer script and the npm package pick them there
  on their own, and the Go launcher now does too instead of pointing at `cargo install`. musl's
  allocator serialises threads, which made a parallel scan over 15 times slower than the glibc
  build, so these builds use mimalloc, which on arm64 brings them level with the glibc build.

### Changed

- **For embedders of `bonsai-core`**, and breaking for code that builds `UnitName` or `WalkCx`
  with a struct literal, which is why this is 0.4.0 rather than 0.3.1:
  - `UnitName` gains a `signature` field. It is joined onto the reported name but never matched
    against a call. `UnitName::with_signature` sets it.
  - `Hooks` gains two methods with defaults:
    - `unit_body`, for a unit kind whose grammar leaves its body unfielded;
    - `call_reaches_unit`, which lets a language with overloading rule out a call that names the
      unit.
  - `WalkCx` gains `unit_node`.
  - Existing languages score exactly as before.

## [0.3.0] - 2026-09-24

### Added

- **Go.** `.go` files are scanned under a new `go` language id, so `--lang go`, `--over go=N` and
  a `[go]` section in the config all work. A method is keyed by its receiver type, so
  `func (s *Stack[T]) Push()` reports as `Stack::Push` and its baseline entry survives switching
  between a pointer and a value receiver. A call through the receiver, `s.Push()`, counts as
  recursion; a bare `Push()` inside the method names a free function or builtin and does not. An
  `if` or `switch` initializer is scored as part of the header, `select` costs one increment like
  a `switch`, and `defer`, `go` and `fallthrough` are free. Package-level function literals are
  units named by their binding, and a file's repeated `init` functions are told apart as `init`,
  `init~2`. Built behind a `go` cargo feature, on by default.

  A file with a `// Code generated … DO NOT EDIT.` line before its `package` clause is skipped,
  neither scored nor counted, on disk and through `--stdin`: it is how the Go toolchain itself
  recognises generated code. `vendor/` and `testdata/` are not special-cased; exclude them if a
  repository commits them.

  Upgrading a repository that contains `.go` files will report findings that were previously
  invisible. Run `bonsai-lint --write-baseline .` to adopt them.

- **Installing through Go.** `go install bonsai.kauneckas.dev/bonsai-lint@latest` installs the
  CLI, and `go get -tool` pins it in a Go module for `go tool bonsai-lint`. The module is a
  launcher with no dependencies, published from
  [bonsai-lint-go](https://github.com/ryckakas/bonsai-lint-go) at every release under the same
  version. The first run of each version downloads that release's binary, checks it against the
  sha256 recorded in the module's source, and caches it. Windows on ARM runs the x64 binary
  under emulation, as the npm package does. musl Linux and glibc older than 2.35 get a message
  pointing at `cargo install`.

### Changed

- The macOS and Linux release archives are `.tar.gz` instead of `.tar.xz`, because the Go
  launcher unpacks them with Go's standard library, which has no xz decoder. The installer
  scripts, npm package and Homebrew formula follow on their own; a script that downloads an
  archive by name needs the new extension.

- For embedders of `bonsai-core` and `bonsai-engine`, the language seam is now two traits with
  default methods, so a new hook no longer forces an edit into every language crate. No score
  moves.
  - `Hooks` is a trait, reached through `LanguageSpec.hooks: &'static dyn Hooks`.
    `unit_scope`, returning a `UnitScope`, replaces `is_self_receiver`: it names the receiver
    spellings through which a call reaches the unit, and whether a bare call does. The new
    `if_parts` lets a language read its own if-chains as `IfPart`s, defaulting to
    `walk::if_parts_by_fields`; the walker keeps the arithmetic.
  - `LanguageDescriptor` is a trait, taken as `&'static dyn LanguageDescriptor` by
    `registry::descriptors`, `registry::for_path` and `Scanner::analyze_source`. Its `id` field
    is gone, and `extract`, `is_generated` and `unscored_suffixes` are defaulted methods.
  - The `.d.ts` and `.min.js` suffix lists moved from the engine to the TypeScript and TSX
    descriptors.
- `--help` no longer names languages in its description, and `--lang` lists the ids built into
  the binary, so a build with fewer language features no longer offers ones it cannot scan.
- In PHP and TypeScript, an IIFE whose result is bound, such as `const config = (() => { … })()`
  or `$config = (function () { … })()`, now takes that binding's name instead of `<anonymous>`,
  and a marker above the binding suppresses it, as Go does. A bare IIFE still has nothing to be
  named after and stays `<anonymous>`, numbered only among the units left anonymous, so adding a
  bound one no longer shifts its key. Such a unit's baseline key changes: until
  `--write-baseline` is rerun, its old entry reports as stale and the unit as new.
- Two domains with the same name are now an error naming both, where they used to be merged
  under one label in silence: `--domain` and the report's `domain` field could not tell them
  apart. That includes a domain named `root`, which is the workspace root's own name.
  Embedders see this as a new `ConfigError::Invalid` variant.

### Fixed

- A suppression marker above a declaration whose function is wrapped in a call, such as
  `const useCart = defineStore('cart', () => …)` or `$handler = wrap(function () { … })`, now
  suppresses that function. The unit was already named after the declaration, but the marker
  search stopped at the call, so the marker was ignored. A marker directly above the callback,
  inside the argument list, keeps working, and now does in PHP too. A call that binds nothing,
  such as `Route::get('/x', function () { … })`, still leaves a marker above it to the file.
- A misspelt top-level key in `bonsai-lint.toml` is reported by name, as
  ``unknown key `treshold`; did you mean `threshold`?``, instead of as "expected struct
  LanguageSection". A misspelt language section, `[typscript]`, gets the same suggestion in its
  warning.

## [0.2.1] - 2026-09-22

### Changed

- Documentation only, with no change to any score.

## [0.2.0] - 2026-09-21

### Added

- Minified JavaScript is skipped, the way `*.d.ts` already was: a bundle's nesting rolls up into
  one unit whose score outranks every real finding, and it is generated output nobody is going
  to refactor. The skipped names now live in one list of whole suffixes, which also closes a gap
  where `*.d.mts` and `*.d.cts` were scanned despite being declaration files. Names that merely
  resemble one, such as `app.mini.js`, are untouched. A baseline holding a newly skipped file
  will report that entry as stale.

- **Vue single-file components.** `.vue` files are scanned under a new `vue` language id, so
  `--lang vue`, `--over vue=N` and a `[vue]` section in the config all work. Both `<script>` and
  `<script setup>` are scored, with `lang="ts"` selecting the TypeScript grammar and anything
  else the JSX-capable one; each block is parsed on its own and their file-level code is added
  into the single `<toplevel>` a component reports. Reported lines are lines in the `.vue` file,
  so editing a template moves a finding without changing its baseline key.

  Templates, `<style>` and custom blocks such as `<docs>` are not scored, and a `src=` block is
  scored as whatever file it points at. A `render()` function written in a script block is
  ordinary code and is scored. Counting template branching would make a component incomparable
  with the same logic written in TypeScript. Built behind a `vue` cargo feature, on by default
  and implying `ts`.

  Upgrading a repository that contains `.vue` files will report findings that were previously
  invisible. Run `bonsai-lint --write-baseline .` to adopt them.

## [0.1.1] - 2026-09-20

### Added

- `-j`, `--jobs <N>` bounds how many files are scored at once. Absent or `0` asks the machine,
  which respects cgroup quotas and CPU affinity, so a CI container gets the number it is actually
  allowed. The request is capped at four times the machine's parallelism.

### Changed

- Files are read, parsed and scored in parallel. On a 1.26 million line monorepo and a ten-core
  machine, a scan drops from **3.16s to 0.80s**, about 1.6 million lines per second. Scaling
  flattens near 4x rather than approaching 10x because six of those cores are efficiency cores,
  and the wall-clock win costs roughly 74% more total CPU.
- Output is unchanged. A report is byte for byte identical to the previous release's at every
  thread count, stdout and stderr alike: files are scored out of order but replayed in walk
  order, and findings are ranked after the scan rather than printed as they arrive.

### Notes

- The scorer is untouched — no change to `bonsai-core`, so no score moves.
- Discovery is still single-threaded, and is now most of what remains. See
  [ROADMAP.md](ROADMAP.md) for the measurement and the two follow-ups it motivates.

## [0.1.0] - 2026-09-19

Initial release.

- Cognitive complexity scoring for PHP, JavaScript and TypeScript, read through tree-sitter
  without executing any of it, from one static binary with no PHP or Node runtime required.
- The same logic scores the same in every supported language, so one threshold is meaningful
  across a mixed repository.
- Monorepo support: domains with their own configs, thresholds, excludes and baselines.
- Baselines to accept existing findings so only later regressions fail, per domain or shared.
- `bonsai-lint-ignore` suppression markers, per unit or per file, with a required reason.
- Text and JSON output, and `--stdin` for scoring an unsaved editor buffer.
- Published as a GitHub release, an npm package, and a Homebrew formula, alongside a VS Code
  extension versioned independently.

[Unreleased]: https://github.com/ryckakas/bonsai-lint/compare/v0.4.3...HEAD
[0.4.3]: https://github.com/ryckakas/bonsai-lint/compare/v0.4.2...v0.4.3
[0.4.2]: https://github.com/ryckakas/bonsai-lint/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/ryckakas/bonsai-lint/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/ryckakas/bonsai-lint/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/ryckakas/bonsai-lint/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/ryckakas/bonsai-lint/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/ryckakas/bonsai-lint/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/ryckakas/bonsai-lint/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ryckakas/bonsai-lint/releases/tag/v0.1.0
