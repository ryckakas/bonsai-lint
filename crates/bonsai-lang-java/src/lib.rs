use std::borrow::Cow;
use std::num::NonZeroU16;

use bonsai_core::language::field;
use bonsai_core::naming::compact;
use bonsai_core::{
    Callee, FieldNames, Hooks, IfPart, KindSets, Language, LanguageDescriptor, LanguageSpec,
    UnitName, UnitScope,
};
use tree_sitter::Node;

#[derive(Debug)]
pub struct Java;

pub static JAVA: Java = Java;

impl LanguageDescriptor for Java {
    fn spec(&self) -> &'static LanguageSpec {
        &SPEC
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["java"]
    }

    fn compiled(&self) -> &'static Language {
        compiled()
    }

    fn is_generated(&self, source: &str) -> bool {
        is_generated(source)
    }
}

pub static SPEC: LanguageSpec = LanguageSpec {
    id: "java",
    kinds: KindSets {
        unit: UNIT,
        // `class_body` too, so an enum constant's body or an anonymous class can be named from
        // what holds it; a declaration's own body adds no segment.
        container: &[
            "class_declaration",
            "interface_declaration",
            "enum_declaration",
            "record_declaration",
            "annotation_type_declaration",
            "class_body",
        ],
        nesting_function: UNIT,
        if_statement: &["if_statement"],
        else_if_clause: &[],
        // Java has no else node: `if_parts` reads the statement under the alternative.
        else_clause: &[],
        nesting_control: &[
            "ternary_expression",
            "switch_expression",
            "for_statement",
            "enhanced_for_statement",
            "while_statement",
            "do_statement",
            "catch_clause",
        ],
        jump: &["break_statement", "continue_statement"],
        unconditional_jump: &[],
        logical: &["binary_expression"],
        parenthesis: &["parenthesized_expression"],
        call: &["method_invocation"],
        comment: &["line_comment", "block_comment"],
        // Annotations sit inside `modifiers`, so stepping over it reports the signature line.
        leading_trivia: &["modifiers"],
        preamble: &["package_declaration"],
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
        // `do … while (c)` puts its condition after the body, where position alone would nest it.
        control_header: &["condition", "init", "update", "value"],
    },
    hooks: &JavaHooks,
    optional_kinds: &[],
};

/// Bodyless declarations never reach scoring: an abstract or interface method has no `body`,
/// and `unit_body` answers `None` for it.
const UNIT: &[&str] = &[
    "method_declaration",
    "constructor_declaration",
    "compact_constructor_declaration",
    "lambda_expression",
    "static_initializer",
];

#[derive(Debug)]
struct JavaHooks;

impl Hooks for JavaHooks {
    fn normalize_logical_operator(&self, operator: &str) -> Option<&'static str> {
        normalize_logical_operator(operator)
    }

    fn is_penalized_jump(&self, node: Node<'_>, _src: &[u8]) -> bool {
        is_penalized_jump(node)
    }

    fn resolve_callee<'t>(&self, node: Node<'t>) -> Option<Callee<'t>> {
        resolve_callee(node)
    }

    fn unit_name(&self, node: Node<'_>, src: &[u8]) -> UnitName {
        unit_name(node, src)
    }

    fn unit_scope(&self, _node: Node<'_>, _src: &[u8], container: Option<&str>) -> UnitScope {
        unit_scope(container)
    }

    fn container_name(&self, node: Node<'_>, src: &[u8]) -> Option<String> {
        container_name(node, src)
    }

    fn suppression_anchors<'t>(&self, node: Node<'t>, src: &[u8], visit: &mut dyn FnMut(Node<'t>)) {
        suppression_anchors(node, src, visit);
    }

    fn if_parts<'t>(&self, node: Node<'t>, lang: &Language, visit: &mut dyn FnMut(IfPart<'t>)) {
        if_parts(node, lang, visit);
    }

    fn unit_body<'t>(&self, node: Node<'t>, lang: &Language) -> Option<Node<'t>> {
        unit_body(node, lang)
    }

    fn call_reaches_unit(&self, call: Node<'_>, unit: Node<'_>, src: &[u8]) -> bool {
        call_reaches_unit(call, unit, src)
    }
}

fn compiled() -> &'static Language {
    bonsai_core::compiled_once!({
        SPEC.compile(tree_sitter_java::LANGUAGE.into())
            .unwrap_or_else(|errors| {
                panic!("Java spec does not match the linked grammar: {errors}")
            })
    })
}

/// `&`, `|` and `^` are deliberately absent: on booleans they are logical, but syntax alone
/// cannot tell them from the bitwise operators.
fn normalize_logical_operator(operator: &str) -> Option<&'static str> {
    match operator {
        "&&" => Some("&&"),
        "||" => Some("||"),
        _ => None,
    }
}

/// The label is an unfielded `identifier` child rather than a field, unlike TypeScript's.
fn is_penalized_jump(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    let labelled = node
        .named_children(&mut cursor)
        .any(|child| child.kind() == "identifier");
    labelled
}

/// `super.m()` and `Outer.super.m()` call the parent's method, so `super` is kept as the
/// receiver, where it never matches a unit's own spellings.
fn resolve_callee(node: Node<'_>) -> Option<Callee<'_>> {
    let mut cursor = node.walk();
    let parent_call = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "super");
    Some(Callee {
        receiver: parent_call.or_else(|| node.child_by_field_name("object")),
        name: node.child_by_field_name("name")?,
    })
}

/// A static method is reached through its class's name and an inner class's method through
/// `Outer.this`; `super` is absent because it reaches the parent's method, not this one.
fn unit_scope(container: Option<&str>) -> UnitScope {
    let mut self_receivers = vec![Cow::Borrowed("this")];
    if let Some(last) = container.and_then(|path| path.rsplit("::").next()) {
        self_receivers.push(Cow::Owned(last.to_string()));
        self_receivers.push(Cow::Owned(format!("{last}.this")));
    }
    UnitScope {
        container: None,
        self_receivers,
        bare_call_recurses: true,
    }
}

/// The alternative is either the chain's next `if` or the else branch itself, braced or not.
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
    if field == lang.fields.condition {
        return Some(IfPart::Header(child));
    }
    if field == lang.fields.if_then {
        return Some(IfPart::Then(child));
    }
    (field == lang.fields.if_alternative).then(|| match child.kind() {
        "if_statement" => IfPart::ElseIf(child),
        _ => IfPart::Else(Some(child)),
    })
}

/// `static { … }` holds its block unfielded.
fn unit_body<'t>(node: Node<'t>, lang: &Language) -> Option<Node<'t>> {
    if node.kind() != "static_initializer" {
        return field(node, lang.fields.body);
    }
    let mut cursor = node.walk();
    let block = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "block");
    block
}

/// Overloads share a name, so a call recurses only when its argument count fits the method's
/// own parameters. A constructor, a static block or a lambda is never invoked by a name.
fn call_reaches_unit(call: Node<'_>, unit: Node<'_>, src: &[u8]) -> bool {
    if unit.kind() != "method_declaration" {
        return false;
    }
    let (Some(arguments), Some(parameters)) = (
        call.child_by_field_name("arguments"),
        unit.child_by_field_name("parameters"),
    ) else {
        return false;
    };

    let mut cursor = arguments.walk();
    let given = arguments
        .named_children(&mut cursor)
        .filter(|argument| !is_comment(*argument))
        .count();

    let mut cursor = parameters.walk();
    let mut required = 0;
    let mut variadic = false;
    for parameter in parameters.named_children(&mut cursor) {
        match parameter_type(parameter, src) {
            Some(written) if written.ends_with("...") => variadic = true,
            Some(_) => required += 1,
            None => {}
        }
    }
    if variadic {
        given >= required
    } else {
        given == required
    }
}

fn unit_name(node: Node<'_>, src: &[u8]) -> UnitName {
    match node.kind() {
        "method_declaration" | "constructor_declaration" => {
            declared(node, node.child_by_field_name("parameters"), src)
        }
        "compact_constructor_declaration" => declared(node, record_parameters(node), src),
        "static_initializer" => UnitName::positional("<static>"),
        _ => lambda_name(node, src),
    }
}

/// Keyed by its parameter types as well as its name, since an overload is a separate method
/// that shares the name.
fn declared(node: Node<'_>, parameters: Option<Node<'_>>, src: &[u8]) -> UnitName {
    let Some(name) = node
        .child_by_field_name("name")
        .and_then(|name| text(name, src))
    else {
        return UnitName::anonymous();
    };
    let types: Vec<String> = parameters
        .map(|parameters| {
            let mut cursor = parameters.walk();
            let types = parameters
                .named_children(&mut cursor)
                .filter_map(|parameter| parameter_type(parameter, src))
                .collect();
            types
        })
        .unwrap_or_default();
    UnitName::declared(name).with_signature(format!("({})", types.join(", ")))
}

/// A compact constructor is the canonical one, so its parameters are the record's components.
fn record_parameters(node: Node<'_>) -> Option<Node<'_>> {
    node.parent()?
        .parent()
        .filter(|record| record.kind() == "record_declaration")?
        .child_by_field_name("parameters")
}

/// A receiver parameter (`Outer this`) is not an argument, so it is not part of the key.
fn parameter_type(parameter: Node<'_>, src: &[u8]) -> Option<String> {
    match parameter.kind() {
        "formal_parameter" => {
            let base = simple_type(parameter.child_by_field_name("type")?, src)?;
            let dimensions = parameter
                .child_by_field_name("dimensions")
                .map_or(0, |dimensions| bracket_pairs(dimensions, src));
            Some(base + &"[]".repeat(dimensions))
        }
        "spread_parameter" => Some(simple_type(written_type(parameter)?, src)? + "..."),
        "ERROR" => annotated_varargs(parameter, src),
        _ => None,
    }
}

/// The grammar rejects a type annotation before `...`, as nullness-annotated code writes
/// `String @Nullable ... args`. The type still leads the error, and dropping it would collide
/// with a real overload and re-key once the grammar learns the syntax.
fn annotated_varargs(error: Node<'_>, src: &[u8]) -> Option<String> {
    let variadic = text(error, src)?.contains("...");
    let base = simple_type(written_type(error)?, src)?;
    Some(if variadic { base + "..." } else { base })
}

fn written_type(parameter: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = parameter.walk();
    let written = parameter.named_children(&mut cursor).find(|child| {
        !matches!(
            child.kind(),
            "modifiers" | "variable_declarator" | "marker_annotation" | "annotation"
        ) && !is_comment(*child)
    });
    written
}

/// The type as its simple name, so `java.util.List<String>` and an imported `List<String>` key
/// alike. Type arguments and annotations are dropped: two overloads cannot differ only by them.
fn simple_type(node: Node<'_>, src: &[u8]) -> Option<String> {
    match node.kind() {
        "generic_type" | "annotated_type" => {
            let mut cursor = node.walk();
            let base = node.named_children(&mut cursor).find(|child| {
                !matches!(
                    child.kind(),
                    "type_arguments" | "marker_annotation" | "annotation"
                )
            })?;
            simple_type(base, src)
        }
        "scoped_type_identifier" => {
            let mut cursor = node.walk();
            let last = node
                .named_children(&mut cursor)
                .filter(|child| child.kind() == "type_identifier")
                .last()?;
            text(last, src)
        }
        "array_type" => {
            let element = simple_type(node.child_by_field_name("element")?, src)?;
            let dimensions = bracket_pairs(node.child_by_field_name("dimensions")?, src);
            Some(element + &"[]".repeat(dimensions))
        }
        _ => text(node, src).map(|written| compact(&written)),
    }
}

fn bracket_pairs(dimensions: Node<'_>, src: &[u8]) -> usize {
    text(dimensions, src).map_or(0, |written| written.matches('[').count())
}

fn lambda_name(node: Node<'_>, src: &[u8]) -> UnitName {
    if let Some(name) = bound_name(node, src) {
        return UnitName::bound(name);
    }
    if let Some(name) = positional_name(node, src) {
        return UnitName::positional(name);
    }
    UnitName::anonymous()
}

/// An enum constant's body takes the constant's name, and an anonymous class the name of
/// whatever holds it.
fn container_name(node: Node<'_>, src: &[u8]) -> Option<String> {
    if node.kind() != "class_body" {
        return node
            .child_by_field_name("name")
            .and_then(|name| text(name, src));
    }
    let owner = node.parent()?;
    match owner.kind() {
        "enum_constant" => text(owner.child_by_field_name("name")?, src),
        "object_creation_expression" => {
            bound_name(owner, src).or_else(|| positional_name(owner, src))
        }
        _ => None,
    }
}

/// The unit's own position first, then, when a call hands it a binding, the declaration of that
/// binding.
fn suppression_anchors<'t>(node: Node<'t>, src: &[u8], visit: &mut dyn FnMut(Node<'t>)) {
    let own = anchor(node, false);
    visit(own);
    if bound_name(node, src).is_some() {
        let declaration = anchor(node, true);
        if declaration.id() != own.id() {
            visit(declaration);
        }
    }
}

/// Climbs to the declaration a marker would sit above: `Runnable handler = () -> {}` is
/// suppressed from the line before the field, not the lambda.
fn anchor(node: Node<'_>, through_calls: bool) -> Node<'_> {
    let mut current = node;
    loop {
        let Some(parent) = current.parent() else {
            return current;
        };
        if let Some(call) = binding_call(parent, current).filter(|_| through_calls) {
            current = call;
            continue;
        }
        match parent.kind() {
            "variable_declarator"
            | "assignment_expression"
            | "parenthesized_expression"
            | "cast_expression" => current = parent,
            "field_declaration"
            | "local_variable_declaration"
            | "constant_declaration"
            | "expression_statement" => return parent,
            _ => return current,
        }
    }
}

/// Walks up to whatever the lambda is bound to. Inside a method a lambda rolls up, so only
/// class-level bindings ever reach here: a field, an assignment in an initializer block, or a
/// factory call handing its own binding to a lone callable argument.
fn bound_name(node: Node<'_>, src: &[u8]) -> Option<String> {
    let mut current = node;
    loop {
        let parent = current.parent()?;
        let is_value = |field: &str| {
            parent
                .child_by_field_name(field)
                .is_some_and(|value| value.id() == current.id())
        };
        match parent.kind() {
            "variable_declarator" if is_value("value") => {
                return text(parent.child_by_field_name("name")?, src).filter(|name| name != "_");
            }
            "assignment_expression" if is_value("right") => {
                return text(parent.child_by_field_name("left")?, src).map(|left| compact(&left));
            }
            "argument_list" => current = binding_call(parent, current)?,
            "parenthesized_expression" | "cast_expression" => current = parent,
            _ => return None,
        }
    }
}

/// The call that hands its own binding to `current`: the one it is the lone callable argument
/// of, as in `memoize(() -> …)` or `new Thread(() -> …)`.
fn binding_call<'t>(parent: Node<'t>, current: Node<'t>) -> Option<Node<'t>> {
    if parent.kind() != "argument_list" || !is_sole_callable_argument(parent, current) {
        return None;
    }
    parent.parent().filter(|call| {
        matches!(
            call.kind(),
            "method_invocation" | "object_creation_expression"
        )
    })
}

fn is_sole_callable_argument(arguments: Node<'_>, candidate: Node<'_>) -> bool {
    let mut cursor = arguments.walk();
    let mut callables = arguments
        .named_children(&mut cursor)
        .filter(|argument| is_callable(*argument));

    callables
        .next()
        .is_some_and(|first| first.id() == candidate.id())
        && callables.next().is_none()
}

/// An anonymous class is Java's other closure, so it competes for the binding like a lambda.
fn is_callable(argument: Node<'_>) -> bool {
    match argument.kind() {
        "lambda_expression" | "method_reference" | "method_invocation" => true,
        "object_creation_expression" => {
            let mut cursor = argument.walk();
            let anonymous = argument
                .named_children(&mut cursor)
                .any(|child| child.kind() == "class_body");
            anonymous
        }
        _ => false,
    }
}

/// A callback that is nobody's value takes the call it belongs to plus its argument position:
/// `executor.submit#0`, `new Dispatcher#1`.
fn positional_name(node: Node<'_>, src: &[u8]) -> Option<String> {
    let arguments = node
        .parent()
        .filter(|parent| parent.kind() == "argument_list")?;
    let callee = callee_name(arguments.parent()?, arguments, src)?;

    let mut cursor = arguments.walk();
    let index = arguments
        .named_children(&mut cursor)
        .position(|argument| argument.id() == node.id())?;

    Some(format!("{callee}#{index}"))
}

/// A method's callee is sliced from the source, so a chain keeps its qualifier. A constructor
/// names its type the way a parameter does, so `new a.Foo<>(…)` and `new Foo(…)` key alike.
fn callee_name(call: Node<'_>, arguments: Node<'_>, src: &[u8]) -> Option<String> {
    match call.kind() {
        "method_invocation" => {
            let callee =
                compact(std::str::from_utf8(&src[call.start_byte()..arguments.start_byte()]).ok()?);
            (!callee.is_empty() && !callee.contains(['(', '{'])).then_some(callee)
        }
        "object_creation_expression" => Some(format!(
            "new {}",
            simple_type(call.child_by_field_name("type")?, src)?
        )),
        _ => None,
    }
}

fn is_comment(node: Node<'_>) -> bool {
    matches!(node.kind(), "line_comment" | "block_comment")
}

/// Java has no single convention, but protobuf, Thrift, Avro and `JavaCC` output all say both
/// phrases in the comments before the first line of code.
#[must_use]
pub fn is_generated(source: &str) -> bool {
    let header = header_comments(source).to_lowercase();
    let header = header.split_whitespace().collect::<Vec<_>>().join(" ");
    header.contains("generated") && header.contains("do not edit")
}

fn header_comments(source: &str) -> String {
    let mut header = String::new();
    let mut rest = source.trim_start_matches('\u{feff}');
    loop {
        rest = rest.trim_start();
        let (comment, tail) = if let Some(line) = rest.strip_prefix("//") {
            line.split_once('\n').unwrap_or((line, ""))
        } else if let Some(block) = rest.strip_prefix("/*") {
            block.split_once("*/").unwrap_or((block, ""))
        } else {
            return header;
        };
        header.push_str(comment);
        header.push('\n');
        rest = tail;
    }
}

fn text(node: Node<'_>, src: &[u8]) -> Option<String> {
    node.utf8_text(src).ok().map(ToString::to_string)
}
