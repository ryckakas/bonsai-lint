# Roadmap

[← Back to README](README.md)

Nothing here is promised for a particular release. Items are listed in the order they are
likely to be worth doing, and each one records the measurement that motivated it.

## Parallel scanning

The scan uses exactly one core today. Measured against a 1.26 million line monorepo:

```text
real 3.74  user 3.19  sys 0.50   cores used 0.99
```

On a ten core machine that leaves nine idle. The work is close to embarrassingly parallel:
every file is read, parsed and scored independently, and only the final ranking needs a defined
order. `ignore::WalkBuilder` already exposes `build_parallel()`, so the walk itself is a small
change.

The care is in what surrounds it. Output has to stay byte for byte deterministic regardless of
completion order, which means collecting then sorting rather than printing as results arrive.
The scan statistics that decide whether a run was trustworthy, the file and error counts, have
to be accumulated across threads without losing any. Per-domain baselines resolve per file, so
domain lookup has to be safe to call concurrently.

This is the single largest speed win available and it does not touch the scorer.

## Vue single-file components

`.vue` files are currently invisible. Pointing the CLI at one reports `no supported files
found`, which on a Vue codebase silently excludes a large share of the logic.

The work is extracting the `<script>` and `<script setup>` blocks from the component and
scoring their contents with the existing TypeScript grammar. Two details decide whether it is
usable: the `lang` attribute selects which grammar to use, and every reported line number needs
an offset so it points at the line in the `.vue` file rather than inside the extracted block.
A baseline key that drifts by the length of a template is worse than no support at all.

Open question worth settling before starting: whether template logic scores. A chain of `v-if`
and `v-for` is genuine branching, but it is not what the specification was written against, and
counting it would make Vue scores incomparable with the same logic written in TypeScript.
The starting position should be that only script blocks score.

## Smaller known items

- **A domain cannot opt a language out.** Per-language thresholds work per domain, but there is
  no way to say that a domain is TypeScript only. The workaround is an `exclude` glob.
  Analysed in [docs/features/domain-language-scope.md](docs/features/domain-language-scope.md).
- **Shipping the CLI inside the extension.** The `.vsix` carries no binary today. Bundling the
  npm wrapper costs 23 KB and makes it fetch its own on first use; one package per platform
  costs about 1.6 MB and fetches nothing. Analysed in
  [docs/features/extension-cli-bundling.md](docs/features/extension-cli-bundling.md).
- **Trusted publishing.** Both npm and crates.io now support OIDC from GitHub Actions, which
  would remove the stored tokens that have to be rotated when they expire.
- **A long lived editor server.** The extension currently starts a process per analysis. A
  `--server` mode reusing the `--stdin` input shape would cut the per keystroke cost.
- **Go support.** Around 0.2 MB of additional grammar, and the scorer already generalises.
- **Type aware linting for the extension.** The TypeScript source has no linter beyond `tsc`.
  Its async surface is where `no-floating-promises` would earn its place.
