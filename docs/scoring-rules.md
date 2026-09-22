# Scoring rules

[← Back to README](../README.md)

Three rules: shorthand that doesn't break reading flow is free; **+1** for each break in the
linear flow of the code; **+nesting** for a flow-breaker that sits inside other flow-breakers.

| Construct | Increment | Raises nesting |
| --- | --- | --- |
| `if`, ternary | +1 +nesting | yes |
| `else if` / `elseif`, `else` | +1 flat | yes |
| `switch`, `match` | +1 +nesting (not per arm) | yes |
| `for`, `foreach`, `for…of`, `for…in`, `while`, `do` | +1 +nesting | yes |
| `catch` | +1 +nesting | yes |
| `try`, `finally` | — | no |
| Labelled jump (`break 2`, `break outer`, `goto`) | +1 | no |
| Sequence of like boolean operators | +1 per run | no |
| Direct recursion | +1 | no |
| Closure, arrow function, nested function | — | yes |
| Class, interface, trait, enum, namespace, object literal | — | no |

`else if` takes a flat increment deliberately: a long chain reads linearly, so penalising it for
depth would misrepresent it.

A loop's header — the `for` initialiser and update, the `foreach` subject, a `while` condition —
is read before the body, so it sits at the loop's own nesting level rather than one deeper.

Boolean operators cost per *run*, not per operator — the cost is in the switching:

```php
$a && $b && $c              // +1  one run
$a && $b || $c              // +2  two runs
$a && $b || $c && $d        // +3  three runs
$a && ($b && $c)            // +1  parentheses are skipped, not treated as a boundary
$a && !($b && $c)           // +2  a negation is not a logical expression, so it ends the run
```

Grouping alone does not start a new run, which is why `!A && (B || C) && D` costs **3**: reading
it, you switch operator mode three times.

## Nesting compounds across function boundaries

A closure scores nothing itself but raises the nesting level, and its cost lands on the unit
that contains it:

```ts
function outer() {
  const run = () => {      // +0, but the nesting level is now 1
    if (condition) { }     // +2, nesting = 1
  };
}                          // 2, attributed to outer
```

The practical consequence, and the reason it matters most in JavaScript:

```js
if (a) { for (const x of xs) { xs.forEach(item => { if (b) { … } }); } }
```

That is **7** — one function, visibly a pyramid. Scoring each function independently from zero
would report two easy functions at 3 and 1, and callback pyramids would cost nothing.

## Units

A function-like is a scoring unit when **no other function-like encloses it in the same file**.
That includes closures: a PHP routes file made of `Route::get(..., function () {})` calls scores
one unit per route, exactly as its Express equivalent does. Anything nested inside rolls up. A
declaration without a body — an abstract or interface method, a TypeScript signature — is not a
unit. Whatever is left over at file scope — procedural code, templates, route tables,
module-level bootstrap, the values of a configuration object — is scored as a single
`<toplevel>` unit per file, and reported only when it scores above zero.

A unit is reported on its signature line, below any `#[Attribute]` or `@decorator`. The
`<toplevel>` unit is reported on line 1, and a suppression marker for it lives in the comment
block at the top of the file, behind the open tag or shebang.

A `.vue` file is the exception to both. Only its script blocks are parsed, so `<toplevel>` is
reported at the start of the first one rather than line 1, and a marker above `<template>` is
outside every parsed region and does nothing at all. Put it at the top of a script block; a
marker in any block suppresses the one `<toplevel>` the component reports.

Units are named from wherever they are bound, since most closures are anonymous where they are
written:

| TypeScript | Unit key |
| --- | --- |
| `function parse() {}` | `parse` |
| `const handler = () => {}` | `handler` |
| `class C { method() {} }` | `C::method` |
| `class F { field = () => {} }` | `F::field` |
| `const api = { onClick() {} }` | `api::onClick` |
| `export default function () {}` | `default` |
| `const useCart = defineStore('cart', () => {})` | `useCart` |
| `app.get('/x', (req, res) => {})` | `app.get#1` |
| anything else | `<anonymous>` |

| PHP | Unit key |
| --- | --- |
| `function parse() {}` | `parse` |
| `$handler = function () {}` | `handler` |
| `$this->handler = function () {}` | `$this->handler` |
| `class C { public function m() {} }` | `C::m` |
| `$api = ['onClick' => function () {}]` | `onClick` |
| `$handler = Closure::fromCallable(function () {})` | `handler` |
| `Route::get('/x', function () {})` | `Route::get#1` |
| `array_map(fn ($x) => $x, $xs)` | `array_map#0` |
| anything else | `<anonymous>` |

A factory or wrapper call hands its own binding to the callback, so a Pinia store or a
`React.memo(...)` component is named after the thing it is assigned to. Only a lone callable
argument is unwrapped — a call that is nobody's value, such as `app.get('/x', fn)` or
`describe('…', fn)`, keeps a positional key of `<callee>#<zero-based argument index>`.

Keys never contain line numbers, so editing above a function does not invalidate its baseline
entry. Where two positional or anonymous keys collide, the later one gains a `~2` suffix.

## Language specifics

**PHP**

- `match` (8.0) is treated as `switch`: one increment for the whole expression.
- `and` / `or` normalise onto `&&` / `||` for run-counting; `xor` is its own operator.
- `elseif` and `else if` score identically, despite different parse shapes.
- `break N` / `continue N` are PHP's analogue of the labelled break.
- Recursion is detected through direct syntactic self-reference (`f()`, `$this->f()`,
  `self::f()`, `static::f()`). Dispatch through a variable needs symbol resolution and is not
  guessed at.

**JavaScript and TypeScript**

- `??`, `?.`, and `&&=` / `||=` / `??=` are shorthand and cost nothing.
- `in` and `instanceof` are comparisons, not flow breaks.
- JSX short-circuit rendering `{cond && <X/>}` costs +1, exactly as `{cond ? <X/> : null}` does.
- `for…of` and `for…in` are one node kind in the grammar and score the same.
- Bodyless declarations — `method_signature`, `abstract_method_signature`, `function_signature`,
  and the various type-level signatures — are not units.
- `.js` is parsed with the TypeScript grammar. Flow-annotated `.js` will misparse.
- `super.f()` and `ClassName.f()` count as self-reference for recursion, as `this.f()` does.

## Choices that move the numbers

Implementations of cognitive complexity make different choices in a handful of places, and
those choices change the numbers. Here is every one bonsai-lint makes, so a score that doesn't
match another tool has an explanation rather than a mystery.

The first is the one that moves numbers: bonsai-lint scores a nested function at the depth it sits
at and rolls it into the unit that contains it, where most implementations score each
function from zero on its own. Per-function says how hard each piece is in isolation;
rolling up says how hard the whole thing is to read in place.

| | Elsewhere | bonsai-lint |
| --- | --- | --- |
| Closure inside a function | scored separately, from zero | carries the nesting it sits at |
| `??`, `a?.b` | +1 | free |
| JSX `{cond && <X/>}` | exempt | +1, the same as the equivalent ternary |
| `const x = a \|\| []` | exempt | +1 |
| Code outside any function | not scored | scored as `<toplevel>` |

## How these rules are pinned

Every worked example above is also a fixture, under `crates/bonsai-lang-php/tests/` and
`crates/bonsai-lang-ts/tests/`, asserted against a stated total — so a scoring change that
contradicts this document fails the build rather than quietly rewriting it. The two cases that
tell competing readings of a boolean run apart each carry their own test:

```text
if (a && (b || c) || d)   // 3  grouping is transparent: one && run, then one || run
if (a && !(b && c))       // 3  a negation is not a logical expression, so it ends the run
```

No other tool is used as an oracle, deliberately. Scoring a nested function in place rather than
from zero makes golden output from a per-function implementation differ on every function that
contains a closure — in JavaScript, most of them. Agreement would prove nothing and
disagreement would prove nothing either, so the fixtures stand on their stated totals instead.
