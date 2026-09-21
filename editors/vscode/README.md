# bonsai-lint for VS Code

Flags functions that are hard to read, across PHP, JavaScript, TypeScript and Vue. Powered by
[bonsai-lint](https://github.com/ryckakas/bonsai-lint), a single Rust binary, with no PHP
runtime and no Node runtime involved in the analysis.

![A cognitive complexity warning underlining a TypeScript function name, with the Problems panel showing one diagnostic](https://github.com/ryckakas/bonsai-lint/raw/HEAD/editors/vscode/images/diagnostic.png)

## What it does

Reports one warning per function that scores above the threshold, on the declaration line.
Cognitive complexity measures how hard code is to *read*, where cyclomatic complexity measures
how hard it is to *test*.

It analyses the buffer **as you type**, not the file on disk, so a diagnostic appears before you
save. Analysis is debounced, and each file is scored in a single short-lived process.

## It sees code other tools miss

Procedural scripts, templates, route files and module-level bootstrap are scored too, reported
as `<toplevel>`, rather than skipped for living outside a function.

![A cognitive complexity warning on a PHP template whose logic sits at file scope, reported as <toplevel>](https://github.com/ryckakas/bonsai-lint/raw/HEAD/editors/vscode/images/toplevel.png)

## It agrees with CI, by design

The extension deliberately does **not** pass a threshold unless you set one. Your repository's
`bonsai-lint.toml`, its per-domain thresholds, its excludes, its baselines and your
`bonsai-lint-ignore` markers all apply exactly as they do on the command line. What you see in
the editor is what the build will fail on. Nothing more, nothing less.

Anything that goes wrong, a broken `bonsai-lint.toml` or a binary that will not start, is written
to the `bonsai-lint` output channel, and the first occurrence of each problem pops up once.

## Settings

| Setting | Default | What it does |
| --- | --- | --- |
| `bonsai-lint.enable` | `true` | Report cognitive complexity. |
| `bonsai-lint.languages` | all six | Editor language ids to analyse. |
| `bonsai-lint.path` | `""` | Path to a binary, absolute or relative to the workspace folder. Empty uses the bundled one, then `PATH`. |
| `bonsai-lint.threshold` | unset | Force one whole-number threshold. **Leave unset** so the repository wins. |

## Requirements

VS Code 1.90 or newer. Nothing else: the extension carries its own `bonsai-lint` and fetches
the build for your platform the first time it analyses a file.

To use a CLI you already have instead, point `bonsai-lint.path` at it. That also covers the
case where the first-run download cannot reach GitHub, behind a proxy for instance. A binary on
`PATH` is used as a last resort. The extension tells you once if it cannot find any of them.

## License

MIT
