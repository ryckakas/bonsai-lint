use std::num::NonZeroU16;

use bonsai_core::naming::{compact, strip_quotes};
use bonsai_core::{
    Callee, FieldNames, Hooks, IfPart, KindSets, Language, LanguageDescriptor, LanguageSpec,
    UnitName, UnitScope,
};
use tree_sitter::Node;

#[derive(Debug)]
pub struct Go;

pub static GO: Go = Go;

impl LanguageDescriptor for Go {
    fn spec(&self) -> &'static LanguageSpec {
        &SPEC
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["go"]
    }

    fn compiled(&self) -> &'static Language {
        compiled()
    }

    fn is_generated(&self, source: &str) -> bool {
        is_generated(source)
    }
}

pub static SPEC: LanguageSpec = LanguageSpec {
    id: "go",
    kinds: KindSets {
        unit: &["function_declaration", "method_declaration", "func_literal"],
        container: &[],
        nesting_function: &["func_literal"],
        if_statement: &["if_statement"],
        else_if_clause: &[],
        // Go has no else node: `if_parts` reads the block or `if` under the alternative.
        else_clause: &[],
        nesting_control: &[
            "for_statement",
            "expression_switch_statement",
            "type_switch_statement",
            "select_statement",
        ],
        jump: &["break_statement", "continue_statement"],
        unconditional_jump: &["goto_statement"],
        logical: &["binary_expression"],
        parenthesis: &["parenthesized_expression"],
        call: &["call_expression"],
        comment: &["comment"],
        leading_trivia: &[],
        preamble: &["package_clause"],
    },
    fields: FieldNames {
        name: "name",
        body: "body",
        condition: "condition",
        if_then: "consequence",
        if_alternative: "alternative",
        else_body: None,
        logical_left: "left",
        logical_right: "right",
        logical_operator: "operator",
        // The switches have no body field, so their header is known by field name alone.
        control_header: &["initializer", "condition", "value", "alias"],
    },
    hooks: &GoHooks,
    optional_kinds: &[],
};

#[derive(Debug)]
struct GoHooks;

impl Hooks for GoHooks {
    fn normalize_logical_operator(&self, operator: &str) -> Option<&'static str> {
        normalize_logical_operator(operator)
    }

    fn is_penalized_jump(&self, node: Node<'_>, src: &[u8]) -> bool {
        is_penalized_jump(node, src)
    }

    fn resolve_callee<'t>(&self, node: Node<'t>) -> Option<Callee<'t>> {
        resolve_callee(node)
    }

    fn unit_name(&self, node: Node<'_>, src: &[u8]) -> UnitName {
        unit_name(node, src)
    }

    fn unit_scope(&self, node: Node<'_>, src: &[u8], _container: Option<&str>) -> UnitScope {
        unit_scope(node, src)
    }

    fn suppression_anchor<'t>(&self, node: Node<'t>) -> Node<'t> {
        suppression_anchor(node)
    }

    fn if_parts<'t>(&self, node: Node<'t>, lang: &Language, visit: &mut dyn FnMut(IfPart<'t>)) {
        if_parts(node, lang, visit);
    }
}

/// `if err := f(); err != nil` runs its initializer beside the condition, so both are header.
/// The alternative is either the chain's next `if` or the else block itself.
fn if_parts<'t>(node: Node<'t>, lang: &Language, visit: &mut dyn FnMut(IfPart<'t>)) {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return;
    }
    loop {
        if let Some(part) = if_part(cursor.field_id(), cursor.node(), lang) {
            visit(part);
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn if_part<'t>(field: Option<NonZeroU16>, child: Node<'t>, lang: &Language) -> Option<IfPart<'t>> {
    if field.is_none() || !child.is_named() {
        return None;
    }
    if field == lang.fields.if_then {
        return Some(IfPart::Then(child));
    }
    if field == lang.fields.if_alternative {
        return Some(match child.kind() {
            "if_statement" => IfPart::ElseIf(child),
            "block" => IfPart::Else(Some(child)),
            _ => IfPart::Unrecognised(child),
        });
    }
    (field == lang.fields.condition || lang.is_header_field(field)).then_some(IfPart::Header(child))
}

/// Go's convention (<https://go.dev/s/generatedcode>) as `go/ast.IsGenerated` reads it: a
/// `// Code generated … DO NOT EDIT.` line before the first line of code.
#[must_use]
pub fn is_generated(source: &str) -> bool {
    let mut in_block = false;
    for line in source.trim_start_matches('\u{feff}').lines() {
        let rest = if in_block {
            line.split_once("*/").map(|(_, after)| after)
        } else {
            Some(line)
        };
        let Some(rest) = rest else {
            continue;
        };
        in_block = false;
        match header_line(rest.trim()) {
            Header::Marker => return true,
            Header::Comment => {}
            Header::OpenBlock => in_block = true,
            Header::Code => return false,
        }
    }
    false
}

enum Header {
    Marker,
    Comment,
    OpenBlock,
    Code,
}

fn header_line(line: &str) -> Header {
    if line
        .strip_prefix("// Code generated ")
        .is_some_and(|rest| rest.ends_with(" DO NOT EDIT."))
    {
        return Header::Marker;
    }
    if line.is_empty() || line.starts_with("//") {
        return Header::Comment;
    }
    match line.strip_prefix("/*") {
        Some(block) => block.find("*/").map_or(Header::OpenBlock, |end| {
            header_line(block[end + 2..].trim())
        }),
        None => Header::Code,
    }
}

fn compiled() -> &'static Language {
    bonsai_core::compiled_once!({
        SPEC.compile(tree_sitter_go::LANGUAGE.into())
            .unwrap_or_else(|errors| panic!("Go spec does not match the linked grammar: {errors}"))
    })
}

fn normalize_logical_operator(operator: &str) -> Option<&'static str> {
    match operator {
        "&&" => Some("&&"),
        "||" => Some("||"),
        _ => None,
    }
}

/// The label is a child rather than a field, so the TypeScript check cannot be reused.
fn is_penalized_jump(node: Node<'_>, _src: &[u8]) -> bool {
    let mut cursor = node.walk();
    let labelled = node
        .named_children(&mut cursor)
        .any(|child| child.kind() == "label_name");
    labelled
}

fn resolve_callee(node: Node<'_>) -> Option<Callee<'_>> {
    let callee = node.child_by_field_name("function")?;
    match callee.kind() {
        "identifier" => Some(Callee {
            receiver: None,
            name: callee,
        }),
        "selector_expression" => Some(Callee {
            receiver: callee.child_by_field_name("operand"),
            name: callee.child_by_field_name("field")?,
        }),
        _ => None,
    }
}

fn unit_name(node: Node<'_>, src: &[u8]) -> UnitName {
    if let Some(name) = node
        .child_by_field_name("name")
        .and_then(|name| text(name, src))
    {
        return if is_repeatable(node, &name) {
            UnitName::positional(name)
        } else {
            UnitName::declared(name)
        };
    }
    if let Some(name) = bound_name(node, src) {
        return UnitName::bound(name);
    }
    if let Some(name) = positional_name(node, src) {
        return UnitName::positional(name);
    }
    UnitName::anonymous()
}

/// Go lets a file declare `init` and `_` any number of times; a declared name is never
/// disambiguated, so these would otherwise collide as one baseline key.
fn is_repeatable(node: Node<'_>, name: &str) -> bool {
    name == "_" || (name == "init" && node.kind() == "function_declaration")
}

/// `func (s *Stack[T]) Push()` is `Stack::Push`. It reaches itself through `s`, or through its
/// type as the method expression `Stack.Push(s)`; a bare `Push()` is some other function.
fn unit_scope(node: Node<'_>, src: &[u8]) -> UnitScope {
    let Some((type_name, binding)) = receiver(node, src) else {
        return UnitScope {
            container: None,
            self_receivers: Vec::new(),
            bare_call_recurses: true,
        };
    };
    let mut self_receivers: Vec<_> = binding.into_iter().map(Into::into).collect();
    self_receivers.push(type_name.clone().into());
    UnitScope {
        container: Some(type_name),
        self_receivers,
        bare_call_recurses: false,
    }
}

/// The receiver's type name and, unless it is blank, its binding.
fn receiver(node: Node<'_>, src: &[u8]) -> Option<(String, Option<String>)> {
    let receiver = node.child_by_field_name("receiver")?;
    let mut cursor = receiver.walk();
    let parameter = receiver
        .named_children(&mut cursor)
        .find(|child| child.kind() == "parameter_declaration")?;

    let type_name = receiver_type(parameter.child_by_field_name("type")?, src)?;
    let binding = parameter
        .child_by_field_name("name")
        .and_then(|name| text(name, src))
        .filter(|name| name != "_");
    Some((type_name, binding))
}

fn receiver_type(node: Node<'_>, src: &[u8]) -> Option<String> {
    match node.kind() {
        "type_identifier" => text(node, src),
        "pointer_type" | "parenthesized_type" => receiver_type(node.named_child(0)?, src),
        "generic_type" => receiver_type(node.child_by_field_name("type")?, src),
        _ => None,
    }
}

/// Climbs to the declaration a marker would sit above: `var handler = func() {}` is suppressed
/// from the line before `var`, and a map entry from the line before its key.
fn suppression_anchor(node: Node<'_>) -> Node<'_> {
    let mut current = node;
    loop {
        let Some(parent) = current.parent() else {
            return current;
        };
        match parent.kind() {
            "expression_list"
            | "literal_element"
            | "keyed_element"
            | "parenthesized_expression" => {
                current = parent;
            }
            // Inside `var ( … )` each spec is a declaration line of its own.
            "var_spec" if is_grouped(parent) => return parent,
            "var_spec" => current = parent,
            "var_declaration" => return parent,
            _ => return current,
        }
    }
}

fn is_grouped(spec: Node<'_>) -> bool {
    spec.parent()
        .is_some_and(|list| list.kind() == "var_spec_list")
}

/// Walks up to whatever the literal is bound to. Inside a function a literal rolls up, so only
/// package-level bindings ever reach here: a `var`, a composite-literal key, or a factory call
/// handing its own binding to a lone callable argument.
fn bound_name(node: Node<'_>, src: &[u8]) -> Option<String> {
    let mut current = node;
    loop {
        let parent = current.parent()?;
        match parent.kind() {
            "expression_list" => return var_name(parent, current, src),
            "literal_element" => return map_key(parent, src),
            "argument_list" if is_sole_callable_argument(parent, current) => {
                current = parent
                    .parent()
                    .filter(|call| call.kind() == "call_expression")?;
            }
            "parenthesized_expression" => current = parent,
            _ => return None,
        }
    }
}

/// `var a, b = f, g` pairs names and values by position. `_` discards the value, so it is not
/// a name.
fn var_name(values: Node<'_>, value: Node<'_>, src: &[u8]) -> Option<String> {
    let spec = values.parent().filter(|spec| spec.kind() == "var_spec")?;
    let mut cursor = values.walk();
    let index = values
        .named_children(&mut cursor)
        .position(|candidate| candidate.id() == value.id())?;

    let mut cursor = spec.walk();
    let name = spec
        .children_by_field_name("name", &mut cursor)
        .nth(index)?;
    text(name, src).filter(|name| name != "_")
}

fn map_key(element: Node<'_>, src: &[u8]) -> Option<String> {
    let entry = element
        .parent()
        .filter(|entry| entry.kind() == "keyed_element")?;
    let is_value = entry
        .child_by_field_name("value")
        .is_some_and(|value| value.id() == element.id());
    if !is_value {
        return None;
    }
    let key = text(entry.child_by_field_name("key")?, src)?;
    Some(unquote(&key))
}

fn unquote(key: &str) -> String {
    let trimmed = key.trim();
    trimmed
        .strip_prefix('`')
        .and_then(|rest| rest.strip_suffix('`'))
        .map_or_else(|| strip_quotes(trimmed), ToString::to_string)
}

fn is_sole_callable_argument(arguments: Node<'_>, candidate: Node<'_>) -> bool {
    let mut cursor = arguments.walk();
    let mut callables = arguments
        .named_children(&mut cursor)
        .filter(|argument| matches!(argument.kind(), "func_literal" | "call_expression"));

    callables
        .next()
        .is_some_and(|first| first.id() == candidate.id())
        && callables.next().is_none()
}

/// A callback that is nobody's value takes the call it belongs to plus its argument position:
/// `http.HandleFunc#1`.
fn positional_name(node: Node<'_>, src: &[u8]) -> Option<String> {
    let arguments = node
        .parent()
        .filter(|parent| parent.kind() == "argument_list")?;
    let call = arguments
        .parent()
        .filter(|parent| parent.kind() == "call_expression")?;

    let function = call.child_by_field_name("function")?;
    if !matches!(function.kind(), "identifier" | "selector_expression") {
        return None;
    }
    let callee = text(function, src)?;

    let mut cursor = arguments.walk();
    let index = arguments
        .named_children(&mut cursor)
        .position(|argument| argument.id() == node.id())?;

    Some(format!("{}#{index}", compact(&callee)))
}

fn text(node: Node<'_>, src: &[u8]) -> Option<String> {
    node.utf8_text(src).ok().map(ToString::to_string)
}
