use bonsai_core::naming::{compact, strip_quotes};
use bonsai_core::{
    Callee, FieldNames, Hooks, KindSets, Language, LanguageDescriptor, LanguageSpec, UnitName,
};
use tree_sitter::Node;

#[derive(Debug)]
pub struct Php;

pub static PHP: Php = Php;

impl LanguageDescriptor for Php {
    fn spec(&self) -> &'static LanguageSpec {
        &SPEC
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["php", "phtml"]
    }

    fn compiled(&self) -> &'static Language {
        compiled()
    }
}

pub static SPEC: LanguageSpec = LanguageSpec {
    id: "php",
    kinds: KindSets {
        unit: UNIT,
        container: &[
            "class_declaration",
            "interface_declaration",
            "trait_declaration",
            "enum_declaration",
        ],
        nesting_function: UNIT,
        if_statement: &["if_statement"],
        else_if_clause: &["else_if_clause"],
        else_clause: &["else_clause"],
        nesting_control: &[
            "conditional_expression",
            "switch_statement",
            "match_expression",
            "for_statement",
            "foreach_statement",
            "while_statement",
            "do_statement",
            "catch_clause",
        ],
        jump: &["break_statement", "continue_statement"],
        unconditional_jump: &["goto_statement"],
        logical: &["binary_expression"],
        parenthesis: &["parenthesized_expression"],
        call: CALL,
        comment: &["comment"],
        leading_trivia: &["attribute_list"],
        preamble: &["php_tag"],
    },
    fields: FieldNames {
        name: "name",
        body: "body",
        condition: "condition",
        if_then: "body",
        if_alternative: "alternative",
        else_body: Some("body"),
        logical_left: "left",
        logical_right: "right",
        logical_operator: "operator",
        control_header: &["condition", "initialize", "update"],
    },
    hooks: &PhpHooks,
    optional_kinds: &["anonymous_function_creation_expression"],
};

#[derive(Debug)]
struct PhpHooks;

impl Hooks for PhpHooks {
    fn normalize_logical_operator(&self, operator: &str) -> Option<&'static str> {
        normalize_logical_operator(operator)
    }

    fn is_penalized_jump(&self, node: Node<'_>, src: &[u8]) -> bool {
        is_penalized_jump(node, src)
    }

    fn resolve_callee<'t>(&self, node: Node<'t>) -> Option<Callee<'t>> {
        resolve_callee(node)
    }

    fn is_self_receiver(&self, text: &str, container: Option<&str>) -> bool {
        is_self_receiver(text, container)
    }

    fn unit_name(&self, node: Node<'_>, src: &[u8]) -> UnitName {
        unit_name(node, src)
    }

    fn container_name(&self, node: Node<'_>, src: &[u8]) -> Option<String> {
        container_name(node, src)
    }

    fn suppression_anchor<'t>(&self, node: Node<'t>) -> Node<'t> {
        suppression_anchor(node)
    }
}

/// A closure at file scope is a unit like a `function`, so a routes file scores per route
/// exactly as its JavaScript equivalent does.
const UNIT: &[&str] = &[
    "function_definition",
    "method_declaration",
    "anonymous_function",
    "anonymous_function_creation_expression",
    "arrow_function",
];

const CALL: &[&str] = &[
    "function_call_expression",
    "member_call_expression",
    "scoped_call_expression",
];

fn compiled() -> &'static Language {
    bonsai_core::compiled_once!({
        SPEC.compile(tree_sitter_php::LANGUAGE_PHP.into())
            .unwrap_or_else(|errors| panic!("PHP spec does not match the linked grammar: {errors}"))
    })
}

/// `and`/`or` are the same logical operators as `&&`/`||` with different precedence, so they
/// normalise together for run-counting. `??` deliberately falls through to `None`: the spec
/// treats null-coalescing as shorthand that does not break reading flow.
fn normalize_logical_operator(operator: &str) -> Option<&'static str> {
    match operator.to_ascii_lowercase().as_str() {
        "&&" | "and" => Some("&&"),
        "||" | "or" => Some("||"),
        "xor" => Some("xor"),
        _ => None,
    }
}

/// `break 2;` is PHP's analogue of the spec's labelled break; plain `break;` reads linearly, so
/// it costs nothing.
fn is_penalized_jump(node: Node<'_>, src: &[u8]) -> bool {
    node.named_child(0)
        .filter(|level| level.kind() == "integer")
        .and_then(|level| level.utf8_text(src).ok())
        .and_then(|text| text.parse::<u32>().ok())
        .is_some_and(|level| level > 1)
}

fn resolve_callee(node: Node<'_>) -> Option<Callee<'_>> {
    match node.kind() {
        "function_call_expression" => node.child_by_field_name("function").map(|name| Callee {
            receiver: None,
            name,
        }),
        "member_call_expression" => Some(Callee {
            receiver: node.child_by_field_name("object"),
            name: node.child_by_field_name("name")?,
        }),
        "scoped_call_expression" => Some(Callee {
            receiver: node.child_by_field_name("scope"),
            name: node.child_by_field_name("name")?,
        }),
        _ => None,
    }
}

fn is_self_receiver(text: &str, _container: Option<&str>) -> bool {
    matches!(text, "$this" | "self" | "static")
}

fn unit_name(node: Node<'_>, src: &[u8]) -> UnitName {
    if let Some(name) = node
        .child_by_field_name("name")
        .and_then(|name| text(name, src))
    {
        return UnitName::declared(name);
    }
    if let Some(name) = bound_name(node, src) {
        return UnitName::bound(name);
    }
    if let Some(name) = positional_name(node, src) {
        return UnitName::positional(name);
    }
    UnitName::anonymous()
}

fn container_name(node: Node<'_>, src: &[u8]) -> Option<String> {
    node.child_by_field_name("name")?
        .utf8_text(src)
        .ok()
        .map(ToString::to_string)
}

/// Climbs to the statement a marker would sit above, so `$handler = function () {}` can be
/// suppressed from the line before it.
fn suppression_anchor(node: Node<'_>) -> Node<'_> {
    let mut current = node;
    loop {
        let Some(parent) = current.parent() else {
            return current;
        };
        let is_value = |field: &str| {
            parent
                .child_by_field_name(field)
                .is_some_and(|value| value.id() == current.id())
        };
        match parent.kind() {
            "assignment_expression" if is_value("right") => current = parent,
            "expression_statement" | "parenthesized_expression" => current = parent,
            _ => return current,
        }
    }
}

/// Walks up to whatever the closure is bound to. A binder only names the closure when the
/// closure really is its value, so `$x = $c ? $f : $g` names neither branch.
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
            "assignment_expression" if is_value("right") => {
                let left = parent.child_by_field_name("left")?;
                let name = text(left, src)?;
                return Some(match left.kind() {
                    "variable_name" => name.trim_start_matches('$').to_string(),
                    _ => compact(&name),
                });
            }
            "array_element_initializer" => return array_key(parent, current, src),
            // A factory call hands its own binding to a lone callable argument; a call that is
            // nobody's value falls through to a positional key.
            "arguments" if is_sole_callable_argument(parent, current) => {
                current = parent.parent().filter(|call| CALL.contains(&call.kind()))?;
            }
            "argument" | "parenthesized_expression" => current = parent,
            _ => return None,
        }
    }
}

fn array_key(element: Node<'_>, value: Node<'_>, src: &[u8]) -> Option<String> {
    let mut cursor = element.walk();
    let mut children = element.named_children(&mut cursor);
    let key = children.next()?;
    let candidate = children.next()?;
    if candidate.id() != value.id() {
        return None;
    }
    text(key, src).map(|key| strip_quotes(&key))
}

fn is_sole_callable_argument(arguments: Node<'_>, candidate: Node<'_>) -> bool {
    let mut cursor = arguments.walk();
    let mut callables = arguments.named_children(&mut cursor).filter(|argument| {
        argument
            .named_child(0)
            .is_some_and(|inner| UNIT.contains(&inner.kind()) || CALL.contains(&inner.kind()))
    });

    callables
        .next()
        .is_some_and(|first| first.id() == candidate.id())
        && callables.next().is_none()
}

/// A callback that is nobody's value takes the call it belongs to plus its argument position:
/// `Route::get#1`, `array_map#0`. A callee that is itself a call or a closure would make the key
/// as long as the code, so only a plain name or member chain qualifies.
fn positional_name(node: Node<'_>, src: &[u8]) -> Option<String> {
    let argument = node.parent().filter(|parent| parent.kind() == "argument")?;
    let arguments = argument
        .parent()
        .filter(|parent| parent.kind() == "arguments")?;
    let call = arguments
        .parent()
        .filter(|parent| CALL.contains(&parent.kind()))?;

    let callee = std::str::from_utf8(&src[call.start_byte()..arguments.start_byte()]).ok()?;
    let callee = compact(callee);
    if callee.is_empty() || callee.contains(['(', '{']) {
        return None;
    }

    let mut cursor = arguments.walk();
    let index = arguments
        .named_children(&mut cursor)
        .position(|candidate| candidate.id() == argument.id())?;

    Some(format!("{callee}#{index}"))
}

fn text(node: Node<'_>, src: &[u8]) -> Option<String> {
    node.utf8_text(src).ok().map(ToString::to_string)
}
