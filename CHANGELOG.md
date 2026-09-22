# Changelog

[← Back to README](README.md)

Notable changes per release, in the format of [Keep a Changelog](https://keepachangelog.com).
This project follows [Semantic Versioning](https://semver.org); while it is pre-1.0, `0.1.x` is
the compatibility unit, so a backwards-compatible change is a patch bump.

`dist` reads the section matching a tag and uses it as that release's notes, so an entry here is
what a user reads on the GitHub release page.

## [Unreleased]

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
