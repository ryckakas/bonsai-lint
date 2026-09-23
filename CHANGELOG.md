# Changelog

[← Back to README](README.md)

Notable changes per release, in the format of [Keep a Changelog](https://keepachangelog.com).
This project follows [Semantic Versioning](https://semver.org); while it is pre-1.0, `0.1.x` is
the compatibility unit, so a backwards-compatible change is a patch bump.

`dist` reads the section matching a tag and uses it as that release's notes, so an entry here is
what a user reads on the GitHub release page.

## [Unreleased]

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

### Changed

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

### Fixed

- A suppression marker above a declaration whose function is wrapped in a call, such as
  `const useCart = defineStore('cart', () => …)` or `$handler = wrap(function () { … })`, now
  suppresses that function. The unit was already named after the declaration, but the marker
  search stopped at the call, so the marker was ignored. A call that binds nothing, such as
  `Route::get('/x', function () { … })`, still leaves a marker above it to the file.

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

[Unreleased]: https://github.com/ryckakas/bonsai-lint/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/ryckakas/bonsai-lint/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/ryckakas/bonsai-lint/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/ryckakas/bonsai-lint/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ryckakas/bonsai-lint/releases/tag/v0.1.0
