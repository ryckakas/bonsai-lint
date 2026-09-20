# Scoring rules

[← Back to README](../README.md)

Three rules from the [specification](https://www.sonarsource.com/resources/cognitive-complexity/):
shorthand that doesn't break reading flow is free; **+1** for each break in the linear flow of
the code; **+nesting** for a flow-breaker that sits inside other flow-breakers.

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

```java
void myMethod2() {
  Runnable r = () -> {     // +0 but nesting level is now 1
    if (condition1) { }    // +2 nesting=1
  };
}                          // Cognitive Complexity 2
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

## Divergences from eslint-plugin-sonarjs

Implementations of cognitive complexity make different choices in a handful of places, and
those choices change the numbers. Here is every one bonsai-lint makes differently from
`eslint-plugin-sonarjs`, so anyone comparing output can see exactly where a gap comes from.

The first is the one that moves numbers: bonsai-lint scores a nested function at the depth it sits
at and rolls it into the unit that contains it, where `eslint-plugin-sonarjs` scores each
function from zero on its own. Per-function says how hard each piece is in isolation;
rolling up says how hard the whole thing is to read in place.

| | `eslint-plugin-sonarjs` | bonsai-lint |
| --- | --- | --- |
| Closure inside a function | scored separately, from zero | carries the nesting it sits at |
| `??`, `a?.b` | +1 | free |
| JSX `{cond && <X/>}` | exempt | +1, the same as the equivalent ternary |
| `const x = a \|\| []` | exempt | +1 |
| Code outside any function | not scored | scored as `<toplevel>` |

## Where these rules come from

Three primary sources were read directly while building the scorer. Where they disagree, the
specification decides.

**The [specification](https://www.sonarsource.com/resources/cognitive-complexity/)** supplies
the worked examples above. The lambda under *Nesting compounds across function boundaries* is
reproduced from it unchanged, annotation and all: the closure itself is `+0`, it raises the
nesting level, and the resulting **2** is attributed to the enclosing method rather than to the
closure. That single example settles the roll-up question.

**`sonar-java`**, SonarSource's own Java implementation, matches it. Its
`CognitiveComplexityVisitor` raises the nesting level around `visitLambdaExpression` and
`visitClass` and walks an entire method with one visitor, instead of starting a fresh score at
each function boundary. When flattening a boolean sequence it calls
`ExpressionUtils.skipParentheses` — which is where `a && (b || c) || d` costing **2** comes
from. Grouping is transparent to a run; a negation is not, because it is not a logical
expression.

**`eslint-plugin-sonarjs`** pushes a new scope at nesting level 0 for every function and reports
each one independently. That is the single divergence that moves numbers the most, and the
reason for the table above.

The metrics fixture shipped alongside `sonar-java` carries its expected values inline, including
the two cases that tell the readings apart — `a && (b||c) || d // +2` and
`a && b || foo(b && c) // +3`. Those, with the specification's worked examples, are translated
into the fixtures under `crates/bonsai-lang-php/tests/` and `crates/bonsai-lang-ts/tests/` and
checked against their stated totals. `eslint-plugin-sonarjs` is deliberately not used as an
oracle: its golden output would differ on every function that contains a closure, which in
JavaScript is most of them.
