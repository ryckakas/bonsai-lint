# bonsai-lint

<img src="docs/images/cover-hero.png" alt="bonsai-lint — cognitive complexity linter for PHP, JavaScript and TypeScript" width="720">

**Find the code that's hard to read — in seconds, across your whole monorepo.**

A cognitive complexity linter written in Rust. One static binary for PHP, JavaScript and
TypeScript. No PHP runtime, no Node runtime, no Composer entry, nothing added to your project.
Scans **1.25 million lines in 3.5 seconds**.

[![CI](https://github.com/ryckakas/bonsai-lint/actions/workflows/ci.yml/badge.svg)](https://github.com/ryckakas/bonsai-lint/actions/workflows/ci.yml)
[![npm](https://img.shields.io/npm/v/bonsai-lint?color=CB3837&logo=npm&logoColor=white)](https://www.npmjs.com/package/bonsai-lint)
[![dependencies](https://deps.rs/repo/github/ryckakas/bonsai-lint/status.svg)](https://deps.rs/repo/github/ryckakas/bonsai-lint)
![Rust 1.90+](https://img.shields.io/badge/rust-1.90%2B-CE422B)
![License: MIT](https://img.shields.io/badge/license-MIT-blue)

## Why this one

- **One tool for the whole repo.** A PHP backend and a TypeScript frontend score on the same
  metric, in one pass, with one number. The same logic written in either language gets the
  same score — that is the point, and it is [tested](#the-same-code-scores-the-same).
- **No runtime, no plugins, no conflicts.** Nothing to wire into your PHPStan or ESLint setup,
  no plugin versions to keep in step, no Composer entry. A 6 MB binary, 1 MB to download — or
  `npx bonsai-lint` and install nothing at all.
- **Never executes your code.** Syntax-only: no autoloader, no reflection, no module
  resolution. Safe to point at third-party or untrusted source.
- **Correct where the reference JavaScript analyser isn't.** SonarSource ships two
  implementations that disagree with each other; we follow the specification and the Java
  reference. See [Divergences](#divergences-from-eslint-plugin-sonarjs).
- **Sees the code other tools miss.** Procedural scripts, templates, route files and
  module-level initialisation are scored too, not skipped for living outside a function. On one
  production PHP backend that surfaced **321 units no function-based scanner reports** — the
  worst of them scoring **173**.
- **Adoptable on day one.** Baseline your existing violations and gate on regressions, instead
  of being told to fix 400 functions before you can turn it on.

Cognitive complexity measures how hard code is to *read*, where cyclomatic complexity measures
how hard it is to *test*. A `switch` with twenty arms is cyclomatically awful and cognitively
fine; three nested `if`s are the reverse. The metric is
[SonarSource's](https://www.sonarsource.com/resources/cognitive-complexity/).

## Install

```bash
npx bonsai-lint --over 15 src/           # run it without installing anything
npm install -D bonsai-lint               # or pin it in the project
```

```bash
brew install ryckakas/tap/bonsai-lint    # macOS and Linux
curl -LsSf https://github.com/ryckakas/bonsai-lint/releases/latest/download/bonsai-lint-installer.sh | sh
cargo install bonsai-lint                # from source
```

The npm package fetches the prebuilt binary for your platform on install — nothing is compiled,
and Node only launches it. The analysis itself is pure Rust.

## Use it

```bash
bonsai src/                     # fail on anything above 15
bonsai --over 10 src/           # stricter
bonsai --all src/               # every unit, ranked
bonsai --format json src/       # for editors and CI
bonsai --write-baseline src/    # record today's findings, exit 0
bonsai --lang php src/          # one language only
```

```text
  69  packages/billing/src/invoice-mapper.service.ts:47  InvoiceMapperService::mapLineItems
  25  services/api/src/Controller/CheckoutController.php:207  CheckoutController::applyDiscounts
  22  packages/web/src/parser/lexer.js:19  Lexer
```

A clean run prints nothing and exits 0. Exit 1 means a breach — or that the scan was
untrustworthy, because a path could not be read or matched no supported file. A gate that
cannot read what it was pointed at must not report success.

### Languages

| Extensions | Parsed as |
| --- | --- |
| `.php`, `.phtml` | PHP |
| `.ts`, `.mts`, `.cts` | TypeScript |
| `.tsx`, `.jsx`, `.js`, `.mjs`, `.cjs` | TypeScript with JSX |
| `*.d.ts` | skipped — signatures only |

`.js` is parsed with the TypeScript grammar, which accepts a superset of JavaScript. Flow
annotations are the one thing this misparses.

### The same code scores the same

```php
foreach ($orders as $order) {          //  +1
    if ($order->isActive()) {          //  +2
        array_map(function ($item) {   //  +0, nesting is now 3
            if ($item->qty > 0) {      //  +4
                return $item->qty > 10 ? 'bulk' : 'single';   // +5
```

```text
  12  backend/Orders.php:3   OrderRepository::syncLineItems
  12  frontend/orders.ts:1   syncLineItems
```

## Zero config, until you need it

Nothing on disk is required. Defaults are a threshold of 15 for every language, `.gitignore`
respected, top-level code scored.

One `bonsai.toml` at the repository root covers a normal project:

```toml
threshold = 15
exclude   = ["vendor/**", "**/*.generated.ts"]

[typescript]
threshold = 20
```

### Monorepos

Add `domains` and each team owns its own thresholds and its own baseline, without editing a
shared file. The root declares *where* domains may live, so a stray config cannot quietly
create one.

```toml
# bonsai.toml at the repository root
domains   = ["packages/*", "services/*"]
threshold = 15
```

```toml
# packages/web/bonsai.toml
name      = "web"
threshold = 20
```

```
packages/web/.bonsai-baseline.json       # each domain keeps its own
services/billing/.bonsai-baseline.json
```

`--write-baseline` then writes one file per domain and prints what it wrote. Baseline keys are
relative to the domain root, so they survive being checked out anywhere, on any platform.

## Code outside a function is still code

Most complexity tools only score functions and methods, so a 200-line procedural template, a
route table or a module-level bootstrap is invisible to them. bonsai scores whatever is left
over as a `<toplevel>` unit, one per file:

```text
 173  services/api/templates/report/summary.php:1  <toplevel>
  95  services/api/scripts/migrate-tenants.php:1  <toplevel>
```

Files with no top-level logic report nothing, so this costs you no noise. Turn it off with
`--no-toplevel` or `toplevel = false`.

## Baselines

```bash
bonsai --write-baseline src/
```

Records everything currently above the threshold as accepted. Later runs fail only on scores
that got worse, or on units the baseline has never seen. An unknown key is treated as a
regression, never as an acceptance — a renamed function is reported rather than silently
inheriting someone else's amnesty. Entries that match nothing are reported too, so a baseline
cannot quietly rot into a permanent exemption.

## Suppression

```php
// bonsai-ignore: hand-tuned state machine, splitting it hurts more than it helps
function parse(string $input): Ast { /* ... */ }
```

Works above the declaration, inside the docblock, or trailing the signature line, in every
supported language. **A marker without a reason is refused**, reported on stderr, and the
finding stands. Suppression hides a finding but never changes a score, and `--all` always shows
the real number.

## Scoring

Three rules from the [specification](https://www.sonarsource.com/resources/cognitive-complexity/):
shorthand that doesn't break reading flow is free; **+1** for each break in linear flow;
**+nesting** when a flow-breaker sits inside other flow-breakers.

Full table in [docs/scoring-rules.md](docs/scoring-rules.md).

### Divergences from eslint-plugin-sonarjs

SonarSource ships two implementations of its own specification that disagree.
`sonar-java` — the reference for the language the specification was written against — rolls
nested functions up with a nesting increment. `eslint-plugin-sonarjs` scores every function
independently from zero. **We follow the specification and the Java reference.**

| | Specification / `sonar-java` | `eslint-plugin-sonarjs` | bonsai-lint |
| --- | --- | --- | --- |
| Closure inside a function | rolls up, `+nesting` | scored separately from 0 | rolls up |
| `??`, `a?.b` | free — "ignore shorthand" | **+1** | free |
| `a && (b && c)` | 1 — parentheses are skipped | 1 | 1 |
| JSX `{cond && <X/>}` | — | exempt | **+1**, like the ternary |
| `const x = a \|\| []` | — | exempt | **+1** |
| Code outside any function | initialiser blocks scored | not scored | scored as `<toplevel>` |

The rollup difference is the one that matters. Resetting the nesting level at every function
boundary means a callback pyramid costs almost nothing:

```js
if (a) { for (const x of xs) { xs.forEach(item => { if (b) { … } }); } }
```

bonsai scores that **7** — one function, visibly a pyramid. Scoring each function from zero
reports it as two easy functions at 3 and 1. Callback nesting is how JavaScript becomes
unreadable, and a tool that cannot see it has missed the point of measuring JavaScript.

Migrating from `eslint-plugin-sonarjs`? Expect your numbers to move, mostly upward, for these
reasons and no others.

## Editor

The [VS Code extension](editors/vscode) reports diagnostics for all five language IDs. It
analyses the buffer as you type, not the file on disk, and it deliberately does **not** pass a
threshold unless you set one — so the editor shows exactly what CI would fail on, your
`bonsai.toml` and baselines included.

## Performance

| Corpus | Files | Lines | Units scored | Time |
| --- | --- | --- | --- | --- |
| Production monorepo, PHP + JS + TS | 12,145 | 1,254,938 | 47,245 | **3.5s** |

Roughly **360,000 lines per second**, across three languages, in a single pass, on an M-series
Mac. No warm-up, no daemon, no language server — one process, start to finish.

The binary is 6.1 MB and about 1 MB to download. A PHP-only build
(`--no-default-features --features php`) is 3.4 MB, should you ever want it.

## Documentation

- [Scoring rules](docs/scoring-rules.md) — the full increment table and per-language notes
- [Architecture](docs/architecture.md) — the language seam, and how to add a language

## License

MIT
