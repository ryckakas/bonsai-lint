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
| Class, interface, trait, enum, namespace | — | no |

`else if` takes a flat increment deliberately: a long chain reads linearly, so penalising it for
depth would misrepresent it.

Boolean operators cost per *run*, not per operator — the cost is in the switching:

```php
$a && $b && $c              // +1  one run
$a && $b || $c              // +2  two runs
$a && $b && $c || $d || $e  // +3  three runs
$a && ($b && $c)            // +1  parentheses are skipped, not treated as a boundary
$a && !($b && $c)           // +2  a negation is not a logical expression, so it ends the run
```

Parentheses being transparent is what the reference implementation does
(`ExpressionUtils.skipParentheses`), and it is why `!A && (B || C) && D` costs **3**: reading it,
you switch operator mode three times.

## Nesting compounds across function boundaries

A closure scores nothing itself but raises the nesting level, and its cost lands on the unit
that contains it. This is the specification's own worked example:

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
Anything nested inside rolls up. Whatever is left over at file scope — procedural code,
templates, route tables, module-level bootstrap — is scored as a single `<toplevel>` unit per
file, and reported only when it scores above zero.

TypeScript units are named from wherever they are bound, since most are anonymous where they
are written:

| Source | Unit key |
| --- | --- |
| `function parse() {}` | `parse` |
| `const handler = () => {}` | `handler` |
| `class C { method() {} }` | `C::method` |
| `class F { field = () => {} }` | `F::field` |
| `const api = { onClick() {} }` | `api::onClick` |
| `export default function () {}` | `default` |
| `app.get('/x', (req, res) => {})` | `app.get#1` |
| anything else | `<anonymous>` |

Keys never contain line numbers, so editing above a function does not invalidate its baseline
entry. Where two positional or anonymous keys collide, the later one gains a `~2` suffix.

## Language specifics

**PHP**

- `match` (8.0) is treated as `switch`: one increment for the whole expression.
- `and` / `or` normalise onto `&&` / `||` for run-counting; `xor` is its own operator.
- `elseif` and `else if` score identically, despite different parse shapes.
- `break N` / `continue N` are PHP's analogue of the labelled break.
- Recursion is detected through direct syntactic self-reference (`f()`, `$this->f()`,
  `self::f()`). Dispatch through a variable needs symbol resolution and is not guessed at.

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

SonarSource ships two implementations of its own specification that disagree with each other.
`sonar-java` — the reference for the language the specification was written against — rolls
nested functions up with a nesting increment, in `visitLambdaExpression`. `eslint-plugin-sonarjs`
pushes a fresh scope per function at nesting zero. We follow the specification and the Java
reference.

| | Specification / `sonar-java` | `eslint-plugin-sonarjs` | bonsai-lint |
| --- | --- | --- | --- |
| Closure inside a function | rolls up, `+nesting` | scored separately from 0 | rolls up |
| `??`, `a?.b` | free | +1 | free |
| JSX `{cond && <X/>}` | — | exempt | +1 |
| `const x = a \|\| []` | — | exempt | +1 |
| Code outside any function | initialiser blocks scored | not scored | scored as `<toplevel>` |
