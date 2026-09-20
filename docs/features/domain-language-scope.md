# Per-domain language scope

[← Back to the roadmap](../../ROADMAP.md)

**Status:** analysed, not started. Roughly half a day.

Let a domain declare which languages it contains, so a TypeScript package stops being scanned
for PHP and a backend stops being scanned for the handful of JavaScript files that happen to
sit in it.

```toml
# packages/web/bonsai-lint.toml
name = "web"
lang = ["typescript"]
```

## The gap

A domain can already set a threshold per language. It cannot say that a language does not
belong to it at all. `--lang` exists but is global, so it cannot express a policy that differs
between two domains in the same scan.

## What it looks like today

`exclude` covers most of it:

```toml
exclude = ["**/*.js", "**/*.mjs", "**/*.cjs"]
```

That works, and anyone needing this now should use it. Two things are worse about it. It states
the implementation rather than the intent, so a reader has to reconstruct why those globs exist.
And it is a list of extensions to keep in step by hand: the example above is already wrong,
because it misses `.jsx`. A language id covers every extension that routes to it, now and when
a new one is added.

## The field name is constrained

It cannot be `languages`. That name is taken on `ConfigFile` by the `#[serde(flatten)]` map at
`crates/bonsai-engine/src/config.rs:23`, which is what turns unknown top-level keys into
`[php]` and `[typescript]` sections and makes a mistyped key an error rather than a silent
default. `lang` mirrors the CLI flag and stays clear of it.

## What it touches

**`crates/bonsai-engine/src/config.rs`** is the bulk, and it mirrors `exclude` closely enough
to copy its shape: one field on `ConfigFile` beside `exclude` at line 18, one on `Domain`,
threaded through `domain_from` with the same rule that a domain overrides only what it sets.
Validation reuses the existing `warn_unknown_languages` path so an unknown id is refused rather
than quietly matching nothing.

**`crates/bonsai-engine/src/scan.rs`** holds the only real trap. The global filter runs before
the domain is known:

```text
154  if pass.languages.is_some_and(|ids| !wanted(ids)) { return; }
159  if !pass.seen.insert(absolute.clone()) || pass.workspace.is_excluded(&absolute) { return; }
168  pass.outcome.stats.files += 1;
172  let index = pass.workspace.domain_for(&absolute);
```

A per-domain filter needs the domain, so the lookup at 172 has to move up. The ordering matters
more than it looks: line 168 counts the file, and that count feeds the guard that decides
whether a scan was trustworthy enough to report success. Filter after it and a run that skipped
every file still reports as though it read them. The domain has to be resolved after the
`seen` and `exclude` checks, then the language filter applied, and only then the file counted.

**Tests.** Around five: config parsing, inheritance from the root, the scan filter itself, the
interaction with `--lang`, and rejection of an unknown id. For scale, `exclude` carries 24 test
references across `config.rs`, `scan.rs` and `cli.rs`.

## Two decisions to settle first

**How `--lang` and `lang` combine.** Intersection is the predictable reading: both have to
allow a language, so `--lang php` against a TypeScript-only domain yields nothing from it. The
alternative, letting the flag override the config, means a CI invocation can silently switch
back on a language a team deliberately turned off, which is the opposite of what declaring it
was for.

**What an emptied scan means.** There is an existing wart here. `bonsai-lint --lang php
packages/ui` today exits 1 with `no supported files found`, because a file filtered out by
policy is not counted as found. Extend that unchanged to domains and a single TypeScript-only
domain fails an otherwise healthy `--lang php` run across a monorepo. A file skipped by policy
should count as seen and skipped rather than never found. Worth fixing in the same change,
since it is the same confusion reached by a second route.
