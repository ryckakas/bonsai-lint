# bonsai for VS Code

Flags functions that are hard to read, across PHP, JavaScript and TypeScript. Powered by
[bonsai-lint](https://github.com/ryckakas/bonsai-lint) — a single Rust binary, with no PHP
runtime and no Node runtime involved in the analysis.

## What it does

Reports one warning per function that scores above the threshold, on the declaration line.
Cognitive complexity measures how hard code is to *read*, where cyclomatic complexity measures
how hard it is to *test*.

It analyses the buffer **as you type**, not the file on disk, so a diagnostic appears before you
save. Analysis is debounced, and each file is scored in a single short-lived process.

## It agrees with CI, by design

The extension deliberately does **not** pass a threshold unless you set one. Your repository's
`bonsai.toml`, its per-domain thresholds, its baselines and your `bonsai-ignore` markers all
apply exactly as they do on the command line. What you see in the editor is what the build will
fail on — nothing more, nothing less.

## Settings

| Setting | Default | What it does |
| --- | --- | --- |
| `bonsai.enable` | `true` | Report cognitive complexity. |
| `bonsai.languages` | all five | Editor language ids to analyse. |
| `bonsai.path` | `""` | Path to a `bonsai` binary. Empty uses the bundled one, then `PATH`. |
| `bonsai.threshold` | unset | Override every language. **Leave unset** so the repository decides. |

## Requirements

Install the CLI with `brew install ryckakas/tap/bonsai-lint`, or set `bonsai.path` to a binary
you already have. The extension will tell you once if it cannot find one.

## License

MIT
