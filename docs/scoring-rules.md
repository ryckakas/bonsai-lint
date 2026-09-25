# Scoring rules

[← Back to README](../README.md)

Three rules: shorthand that doesn't break reading flow is free; **+1** for each break in the
linear flow of the code; **+nesting** for a flow-breaker that sits inside other flow-breakers.

| Construct | Increment | Raises nesting |
| --- | --- | --- |
| `if`, ternary | +1 +nesting | yes |
| `else if` / `elseif`, `else` | +1 flat | yes |
| `switch`, `match`, `select` | +1 +nesting (not per arm) | yes |
| `for`, `foreach`, `for…of`, `for…in`, `while`, `do` | +1 +nesting | yes |
| `catch` | +1 +nesting | yes |
| `try`, `finally`, `synchronized` | — | no |
| Labelled jump (`break 2`, `break outer`, `goto`) | +1 | no |
| Sequence of like boolean operators | +1 per run | no |
| Direct recursion | +1 | no |
| Closure, arrow function, lambda, nested function, anonymous class method | — | yes |
| Class, interface, trait, enum, record, namespace, object literal | — | no |

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
declaration without a body — an abstract or interface method, a TypeScript signature, a Go
function implemented in assembly — is not a unit. Whatever is left over at file scope —
procedural code, templates, route tables, module-level bootstrap, the values of a configuration
object — is scored as a single `<toplevel>` unit per file, and reported only when it scores
above zero.

A unit is reported on its signature line, below any `#[Attribute]` or `@decorator`. The
`<toplevel>` unit is reported on line 1, and a suppression marker for it lives in the comment
block at the top of the file, behind the open tag, the shebang or Go's `package` clause.

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
| `const config = (() => {})()` | `config` |
| `app.get('/x', (req, res) => {})` | `app.get#1` |
| anything else | `<anonymous>` |

| Java | Unit key |
| --- | --- |
| `class OrderService { void process(Order o, User u) {} }` | `OrderService::process(Order, User)` |
| `<T> void put(@NonNull java.util.Map<K, V> m, T[] xs, String... rest)` | `C::put(Map, T[], String...)` |
| `class Point { Point(int x, int y) {} }` | `Point::Point(int, int)` |
| a compact constructor in `record Point(int x, int y)` | `Point::Point(int, int)` |
| `class Outer { class Inner { void m() {} } }` | `Outer::Inner::m()` |
| `enum Op { PLUS { int apply(int a, int b) {} } }` | `Op::PLUS::apply(int, int)` |
| `static final Comparator<S> CMP = new Comparator<>() { public int compare(S a, S b) {} };` | `C::CMP::compare(S, S)` |
| `private final Runnable handler = () -> {};` | `C::handler` |
| `static final Supplier<X> S = memoize(() -> {});` | `C::S` |
| `new Dispatcher(new Runnable() { public void run() {} }, new Runnable() { … })` | `C::new Dispatcher#0::run()`, `C::new Dispatcher#1::run()` |
| `static { … }`, and a second one | `C::<static>`, `C::<static>~2` |
| `void main() {}` in a compact source file | `main()` |
| anything else | `<anonymous>` |

Java overloads freely, so a method or constructor key carries its parameter types. Each is the
type's simple name, with type arguments and annotations dropped. `java.util.List<String>` and an
imported `List<String>` therefore key alike, and a key survives reordering and adding overloads,
changing only when a parameter type does. Two overloads whose types differ only by package, such
as `java.util.Date` and `java.sql.Date`, share a key.

| PHP | Unit key |
| --- | --- |
| `function parse() {}` | `parse` |
| `$handler = function () {}` | `handler` |
| `$this->handler = function () {}` | `$this->handler` |
| `class C { public function m() {} }` | `C::m` |
| `$api = ['onClick' => function () {}]` | `onClick` |
| `$handler = Closure::fromCallable(function () {})` | `handler` |
| `$config = (function () {})()` | `config` |
| `Route::get('/x', function () {})` | `Route::get#1` |
| `array_map(fn ($x) => $x, $xs)` | `array_map#0` |
| anything else | `<anonymous>` |

| Go | Unit key |
| --- | --- |
| `func parse() {}` | `parse` |
| `func (s *Stack[T]) Push(v T) {}` | `Stack::Push` |
| `var handler = func() {}` | `handler` |
| `var routes = map[string]func(){"list": func() {}}` | `list` |
| `var handler = wrap(func() {})` | `handler` |
| `var cfg = func() T { … }()` | `cfg` |
| `var _ = register(func() {})` | `register#0` |
| a second `func init()` in the same file | `init~2` |
| anything else | `<anonymous>` |

A Go method is declared beside its type rather than inside it, so the receiver supplies the
container: pointer and type parameters are dropped, and the key does not change when the receiver
switches between pointer and value. `init` and `_` may be declared any number of times, so they
are told apart like positional keys, and a `_` binding names nothing.

A factory or wrapper call hands its own binding to the callback, so a Pinia store or a
`React.memo(...)` component is named after the thing it is assigned to. Only a lone callable
argument is unwrapped — a call that is nobody's value, such as `app.get('/x', fn)` or
`describe('…', fn)`, keeps a positional key of `<callee>#<zero-based argument index>`.

Keys never contain line numbers, so editing above a function does not invalidate its baseline
entry. Where two positional or anonymous keys collide, the later one gains a `~2` suffix.

## Language specifics

**JavaScript and TypeScript**

- `??`, `?.`, and `&&=` / `||=` / `??=` are shorthand and cost nothing.
- `in` and `instanceof` are comparisons, not flow breaks.
- JSX short-circuit rendering `{cond && <X/>}` costs +1, exactly as `{cond ? <X/> : null}` does.
- `for…of` and `for…in` are one node kind in the grammar and score the same.
- Bodyless declarations — `method_signature`, `abstract_method_signature`, `function_signature`,
  and the various type-level signatures — are not units.
- `.js` is parsed with the TypeScript grammar. Flow-annotated `.js` will misparse.
- `super.f()` and `ClassName.f()` count as self-reference for recursion, as `this.f()` does.

**Java**

- A `switch` costs one increment in statement and expression form, with `case …:` groups or
  `->` rules alike. `yield` is free, and a `when` guard costs only its operators.
- Each `catch` costs +1 +nesting, and a multi-catch `catch (A | B e)` is one clause. `try`,
  try-with-resources and `finally` are free.
- A labelled `break` or `continue` costs +1; an unlabelled jump is free.
- Only `&&` and `||` form operator runs. `&`, `|` and `^` are free: on booleans they are logical,
  but syntax alone cannot tell them from the bitwise operators.
- `synchronized` neither costs nor nests. `assert`, `throw`, `instanceof` and its patterns, and
  method references are free.
- Lambdas and the methods of anonymous and local classes raise nesting and roll up into the
  method that holds them.
- A `static { … }` block is a unit of its own. Field initialisers and instance initializer
  blocks are the file's `<toplevel>`.
- **Recursion:**
  - What counts: `m()`, `this.m()`, `C.m()` and `C.this.m()`.
  - `super.m()` runs the parent's implementation and is not recursion.
  - Constructor chaining through `this(…)` or `super(…)` is not a call.
  - Overloads share a name, so a call reaches the method only when its argument count fits,
    where varargs accepts any number beyond the fixed parameters. `process(o) { process(o, u); }`
    is a delegating overload, not recursion.
  - An overload with as many parameters still reads as the method itself.
  - A call through another object of the same type, such as `left.count()`, would need type
    information and is not counted.
- A file whose comments before the first line of code say both "generated" and "do not edit" is
  neither scored nor counted. That is how protobuf, Thrift, Avro and JavaCC output marks itself.
- **Grammar gaps.** The grammar predates some Java 21–25 syntax, and these parse with an error:
  - several patterns in one `case` label;
  - `case final`;
  - a qualified record pattern;
  - `import module`;
  - statements before `super(…)` in a constructor;
  - `1__000`.

  The code around the error still scores, but the construct itself may not. A type annotation
  before varargs, `String @Nullable ... args`, parses with an error too, but its method keeps
  both its score and its key, `setHosts(String...)`.

**PHP**

- `match` (8.0) is treated as `switch`: one increment for the whole expression.
- `and` / `or` normalise onto `&&` / `||` for run-counting; `xor` is its own operator.
- `elseif` and `else if` score identically, despite different parse shapes.
- `break N` / `continue N` are PHP's analogue of the labelled break.
- Recursion is detected through direct syntactic self-reference (`f()`, `$this->f()`,
  `self::f()`, `static::f()`). Dispatch through a variable needs symbol resolution and is not
  guessed at.

**Go**

- `for` is the only loop, and its three-clause, condition-only, infinite and `range` forms are one
  node kind that scores the same.
- An expression switch, a type switch and `select` each cost one increment for the whole
  statement. `fallthrough` is free.
- A labelled `break` or `continue`, and `goto`, cost +1; an unlabelled jump is free.
- The initializer in `if err := f(); err != nil` is header, scored at the statement's own depth
  like the condition beside it. The same holds for a `switch` initializer.
- There is no `else` node in the grammar, but an `else if` still scores flat and a plain `else`
  block still nests, as in TypeScript and PHP.
- `defer`, `go` and `recover` are free; a function literal they launch raises nesting like any
  closure.
- Recursion is a call through the receiver, `s.Push()` or `(*s).Push()`, or a method expression
  such as `(*Stack).Push(s)` or `Stack[T].Push(s)`. A method cannot be called without its
  receiver, so a bare `Push()` inside it names a free function or a builtin and is not recursion,
  and neither is `strings.Split` inside `func Split`.
- A call through any other value, even one of the same type such as `c.Walk()` over a node's
  children, would need type information to recognise and is not counted.
- A generic function calling itself counts whether its type arguments are inferred, `Walk(x)`, or
  written out, `Walk[T](x)`, even when they differ from its own: it is still a call to its own
  name.
- A file with a `// Code generated … DO NOT EDIT.` line before its `package` clause is neither
  scored nor counted.

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
| An `if` inside a plain `else` block | no deeper than the `else` | one level deeper, like any branch body |
| `a && (b \|\| c) && d` | 2, the group counted as its own run | 3, grouping is transparent |
| `a && b \|\| c && d` | 2, each operator counted once | 3, each switch of operator starts a run |
| A later link of an `else if` chain | one level deeper per link | one level below its `if`, like the first |
| A loop or `switch` header | one level deeper | at the construct's own depth |
| A method calling itself through its receiver | not recursion | +1 |
| A builtin sharing a method's name, `append()` inside `append` | +1, as recursion | free |
| A local closure sharing its function's name | resolved to the local | +1 per call, matched by name |
| A Java overload taking as many arguments | resolved by type, not recursion | +1, matched by name and argument count |
| A call through another instance of the same type, `left.count()` | +1, as recursion | free |

The last three rows are limits rather than preferences. Recursion is recognised by syntax alone,
with no scope or type analysis. So a closure that shadows its enclosing function's name reads as
a self-call in every language, an overload with the same number of parameters reads as the method
itself, and a call through another object of the same type is not recognised at all.

Every chain body sits one level below its `if` because `else if` is a flat increment. Nesting each
later link deeper would charge a long chain for its length, which is exactly what the flat
increment exists to avoid. A run that returns to an operator after switching away costs again,
as the counting rule states: `a && b || c && d` switches operator twice, so it is three runs.

## How these rules are pinned

Every worked example above is also a fixture, under the language crate's `tests/` directory
(`crates/bonsai-lang-*/tests/`), asserted against a stated total — so a scoring change that
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
