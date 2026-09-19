#!/usr/bin/env node
// a comment
@dec
class Decorated {
    static { init(); }
    method() { return 1; }
}

abstract class Base {
    abstract thing(): void;
}

const Expression = class { inner() { return 1; } };

interface Contract { thing(): void; }

enum Suit { Hearts }

namespace Inner { export const x = 1; }

declare module "external" { export const y: number; }

const literal = { key: function () { return 1; } };

function* generatorDeclaration() { yield 1; }

const generatorExpression = function* () { yield 1; };

const named = function namedExpression() { return 1; };

const arrow = () => 1;

function everything(a: number, b: number) {
    if (a) { f(); } else if (b) { g(); } else { h(); }
    const ternary = a ? 1 : 2;
    switch (a) { case 1: f(); break; default: g(); }
    for (let i = 0; i < 3; i++) { f(); }
    for (const x of [a, b]) { f(); }
    while (a) { break; }
    do { f(); } while (a);
    outer: for (const y of [a]) { continue outer; }
    try { everything(1, 2); } catch (e) { f(); }
    if (a && (b || a)) { f(); }
    return arrow();
}
