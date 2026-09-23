use tree_sitter::Node;

use crate::finding::UnitScope;
use crate::language::{field, Flags, Language, Role};

pub struct WalkCx<'a> {
    pub lang: &'a Language,
    pub src: &'a [u8],
    pub unit: &'a str,
    pub scope: &'a UnitScope,
    /// Set only for the top-level pass. Inside a unit body a nested function-like rolls up, but
    /// at file scope it is a unit in its own right and `collect` already reports it, so counting
    /// it here as well would double it.
    pub skip_units: bool,
}

impl std::fmt::Debug for WalkCx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WalkCx")
            .field("unit", &self.unit)
            .finish_non_exhaustive()
    }
}

#[must_use]
pub fn score_node(node: Node<'_>, cx: &WalkCx<'_>) -> u32 {
    let mut score = 0;
    walk(node, 0, cx, &mut score);
    score
}

#[must_use]
pub fn score_nodes<'t>(nodes: impl Iterator<Item = Node<'t>>, cx: &WalkCx<'_>) -> u32 {
    let mut score = 0;
    for node in nodes {
        walk(node, 0, cx, &mut score);
    }
    score
}

fn walk(node: Node<'_>, nesting: u32, cx: &WalkCx<'_>, score: &mut u32) {
    let info = cx.lang.info(node.kind_id());

    if cx.skip_units {
        if info.flags.has(Flags::UNIT) {
            return;
        }
        // A container's body is file-level code too; only the units inside it are reported apart.
        if info.flags.has(Flags::CONTAINER) {
            walk_children(node, nesting, cx, score);
            return;
        }
    }

    // Must be tested ahead of the role match: a kind can be both a unit and a nesting function,
    // and nesting has to win when it appears inside another unit.
    if info.flags.has(Flags::NESTING_FN) {
        walk_children(node, nesting + 1, cx, score);
        return;
    }

    match info.role {
        Role::If => {
            walk_if(node, nesting, false, cx, score);
            return;
        }
        Role::Control => {
            *score += 1 + nesting;
            walk_control(node, nesting, cx, score);
            return;
        }
        Role::Jump => {
            if cx.lang.spec.hooks.is_penalized_jump(node, cx.src) {
                *score += 1;
            }
            return;
        }
        Role::Goto => {
            *score += 1;
            return;
        }
        Role::Logical => {
            if logical_operator(node, cx).is_some() {
                walk_logical_sequence(node, nesting, cx, score);
                return;
            }
        }
        Role::Call if is_recursive_call(node, cx) => {
            *score += 1;
        }
        _ => {}
    }

    walk_children(node, nesting, cx, score);
}

/// One piece of an `if`, as its language reads the tree. What each piece costs is decided here,
/// in core, so every language scores a chain the same way however its grammar shapes it.
#[derive(Debug, Clone, Copy)]
pub enum IfPart<'t> {
    /// Scored at the if's own depth.
    Header(Node<'t>),
    /// Scored one level deeper.
    Then(Node<'t>),
    /// The next link of the chain, read by `if_parts` in turn and scored a flat +1.
    ElseIf(Node<'t>),
    /// +1, with its body one level deeper.
    Else(Option<Node<'t>>),
    /// Walked at the if's depth. The grammar contract fails on it, naming the kind.
    Unrecognised(Node<'t>),
}

/// `flat` marks an `if` that is really the tail of an `else if`, which the spec scores as a
/// single flat +1 so that long chains aren't punished for depth.
fn walk_if(node: Node<'_>, nesting: u32, flat: bool, cx: &WalkCx<'_>, score: &mut u32) {
    *score += if flat { 1 } else { 1 + nesting };
    cx.lang.spec.hooks.if_parts(node, cx.lang, &mut |part| {
        score_if_part(part, nesting, cx, score);
    });
}

fn score_if_part(part: IfPart<'_>, nesting: u32, cx: &WalkCx<'_>, score: &mut u32) {
    match part {
        IfPart::Header(node) | IfPart::Unrecognised(node) => walk(node, nesting, cx, score),
        IfPart::Then(node) => walk(node, nesting + 1, cx, score),
        IfPart::ElseIf(node) => walk_if(node, nesting, true, cx, score),
        IfPart::Else(None) => *score += 1,
        IfPart::Else(Some(body)) => {
            *score += 1;
            walk(body, nesting + 1, cx, score);
        }
    }
}

/// Reads an if-chain through the spec's field names and else/else-if kinds, which is how a
/// language reads it unless its hooks override `if_parts`. Public so an override can defer to it.
pub fn if_parts_by_fields<'t>(node: Node<'t>, lang: &Language, visit: &mut dyn FnMut(IfPart<'t>)) {
    if let Some(condition) = field(node, lang.fields.condition) {
        visit(IfPart::Header(condition));
    }
    if lang.role(node) == Role::ElseIf {
        if let Some(body) = field(node, lang.fields.body) {
            visit(IfPart::Then(body));
        }
        return;
    }

    if let Some(then) = field(node, lang.fields.if_then) {
        visit(IfPart::Then(then));
    }

    let Some(alternative) = lang.fields.if_alternative else {
        return;
    };
    let mut cursor = node.walk();
    for alt in node.children_by_field_id(alternative, &mut cursor) {
        visit(alternative_part(alt, lang));
    }
}

fn alternative_part<'t>(alt: Node<'t>, lang: &Language) -> IfPart<'t> {
    match lang.role(alt) {
        Role::ElseIf => IfPart::ElseIf(alt),
        Role::Else => else_part(alt, lang),
        _ => IfPart::Unrecognised(alt),
    }
}

/// `else if` written as two words parses as an else clause wrapping an `if`. It must score the
/// same as `elseif`, so the inner `if` is the next link and the `else` adds nothing.
fn else_part<'t>(node: Node<'t>, lang: &Language) -> IfPart<'t> {
    let body =
        field(node, lang.fields.else_body).or_else(|| first_significant_named_child(node, lang));
    match body {
        Some(inner) if lang.role(inner) == Role::If => IfPart::ElseIf(inner),
        body => IfPart::Else(body),
    }
}

/// Branch bodies nest; the controlling header does not. Anything before the body is header too,
/// because a grammar can leave part of a header, such as a loop's subject, without a field name.
/// Every header-field child is exempted, not just the first, since a header field can repeat.
fn walk_control(node: Node<'_>, nesting: u32, cx: &WalkCx<'_>, score: &mut u32) {
    let body_start = field(node, cx.lang.fields.body).map(|body| body.start_byte());

    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return;
    }
    loop {
        let child = cursor.node();
        if child.is_named() {
            let is_header = cx.lang.is_header_field(cursor.field_id())
                || body_start.is_some_and(|start| child.end_byte() <= start);
            walk(child, nesting + u32::from(!is_header), cx, score);
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn walk_children(node: Node<'_>, nesting: u32, cx: &WalkCx<'_>, score: &mut u32) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk(child, nesting, cx, score);
    }
}

/// Comments are tree-sitter extras and therefore named children, so taking `named_child(0)`
/// alone would pick up `else /* why */ { ... }` as the else body.
fn first_significant_named_child<'t>(node: Node<'t>, lang: &Language) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| lang.role(*child) != Role::Trivia);
    found
}

/// A run of like operators costs +1 however long it is; the cost is in the switching.
/// `a && b && c` is +1, `a && b || c` is +2.
fn walk_logical_sequence(node: Node<'_>, nesting: u32, cx: &WalkCx<'_>, score: &mut u32) {
    let mut operators = Vec::new();
    let mut operands = Vec::new();
    flatten_logical(node, cx, &mut operators, &mut operands);

    *score += count_runs(&operators);

    for operand in operands {
        walk(operand, nesting, cx, score);
    }
}

/// Parentheses are skipped rather than treated as sequence boundaries, matching the reference
/// implementation's `skipParentheses`: `a && (b || c) || d` is two runs, not three. A negation
/// or any other non-logical wrapper still ends the sequence, because the wrapper itself is not
/// a logical expression and becomes an operand whose inside is scored separately.
fn flatten_logical<'t>(
    node: Node<'t>,
    cx: &WalkCx<'_>,
    operators: &mut Vec<&'static str>,
    operands: &mut Vec<Node<'t>>,
) {
    if let Some(left) = field(node, cx.lang.fields.logical_left) {
        let left = skip_parentheses(left, cx.lang);
        if logical_operator(left, cx).is_some() {
            flatten_logical(left, cx, operators, operands);
        } else {
            operands.push(left);
        }
    }

    if let Some(operator) = logical_operator(node, cx) {
        operators.push(operator);
    }

    if let Some(right) = field(node, cx.lang.fields.logical_right) {
        let right = skip_parentheses(right, cx.lang);
        if logical_operator(right, cx).is_some() {
            flatten_logical(right, cx, operators, operands);
        } else {
            operands.push(right);
        }
    }
}

fn skip_parentheses<'t>(node: Node<'t>, lang: &Language) -> Node<'t> {
    let mut current = node;
    while lang.flags(current).has(Flags::PARENTHESIS) {
        match first_significant_named_child(current, lang) {
            Some(inner) => current = inner,
            None => break,
        }
    }
    current
}

fn count_runs(operators: &[&str]) -> u32 {
    let mut runs = 0;
    let mut previous: Option<&str> = None;

    for &operator in operators {
        if previous != Some(operator) {
            runs += 1;
            previous = Some(operator);
        }
    }

    runs
}

fn logical_operator(node: Node<'_>, cx: &WalkCx<'_>) -> Option<&'static str> {
    if cx.lang.role(node) != Role::Logical {
        return None;
    }
    let operator = field(node, cx.lang.fields.logical_operator)?
        .utf8_text(cx.src)
        .ok()?;
    cx.lang.spec.hooks.normalize_logical_operator(operator)
}

/// Only direct syntactic self-reference is detectable without symbol resolution; dynamic
/// dispatch through a variable is out of reach and is documented as such rather than guessed at.
fn is_recursive_call(node: Node<'_>, cx: &WalkCx<'_>) -> bool {
    let Some(callee) = cx.lang.spec.hooks.resolve_callee(node) else {
        return false;
    };

    let names_unit = callee
        .name
        .utf8_text(cx.src)
        .is_ok_and(|text| text == cx.unit);

    names_unit
        && match callee.receiver {
            Some(receiver) => receiver
                .utf8_text(cx.src)
                .is_ok_and(|text| cx.scope.self_receivers.iter().any(|name| name == text)),
            None => cx.scope.bare_call_recurses,
        }
}
