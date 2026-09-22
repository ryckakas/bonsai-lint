use bonsai_core::naming::{compact, strip_quotes};
use bonsai_core::{
    Callee, FieldNames, Hooks, KindSets, Language, LanguageDescriptor, LanguageSpec, UnitName,
};
use tree_sitter::Node;

/// `.ts` must not be parsed with the TSX grammar: an angle-bracket type assertion collides with
/// a JSX element.
pub static TYPESCRIPT: LanguageDescriptor = LanguageDescriptor {
    id: "typescript",
    extensions: &["ts", "mts", "cts"],
    spec: &SPEC,
    compiled: compiled_typescript,
    extract: None,
};

/// TSX is a superset of JavaScript and JSX, so it serves `.js` and `.jsx` too and
/// `tree-sitter-javascript` is not a dependency.
pub static TSX: LanguageDescriptor = LanguageDescriptor {
    id: "tsx",
    extensions: &["tsx", "jsx", "js", "mjs", "cjs"],
    spec: &SPEC,
    compiled: compiled_tsx,
    extract: None,
};

/// The two dialects differ only by JSX nodes and `type_assertion`, none of which the scorer
/// references, so one spec serves both. The id is a parameter because an embedded dialect reuses
/// these kinds under a name of its own.
#[must_use]
pub const fn spec(id: &'static str) -> LanguageSpec {
    LanguageSpec {
        id,
        kinds: KindSets {
            unit: UNIT,
            container: &[
                "class_declaration",
                "abstract_class_declaration",
                "class",
                "interface_declaration",
                "enum_declaration",
                "internal_module",
                "module",
                "object",
            ],
            nesting_function: UNIT,
            if_statement: &["if_statement"],
            // TypeScript has no else-if clause: `else if` is a nested `if_statement` inside the
            // alternative, which `walk_else` already handles as the two-word form.
            else_if_clause: &[],
            else_clause: &["else_clause"],
            nesting_control: &[
                "ternary_expression",
                "switch_statement",
                "for_statement",
                "for_in_statement",
                "while_statement",
                "do_statement",
                "catch_clause",
            ],
            jump: &["break_statement", "continue_statement"],
            unconditional_jump: &[],
            logical: &["binary_expression"],
            parenthesis: &["parenthesized_expression"],
            call: &["call_expression"],
            comment: &["comment"],
            leading_trivia: &["decorator"],
            preamble: &["hash_bang_line"],
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
            control_header: &[
                "condition",
                "value",
                "initializer",
                "increment",
                "left",
                "right",
            ],
        },
        hooks: Hooks {
            normalize_logical_operator,
            is_penalized_jump,
            resolve_callee,
            is_self_receiver,
            unit_name,
            container_name,
            suppression_anchor,
        },
        optional_kinds: &[],
    }
}

pub static SPEC: LanguageSpec = spec("typescript");

/// Bodyless declarations are deliberately absent: `method_signature`,
/// `abstract_method_signature`, `function_signature`, `call_signature`, `construct_signature`,
/// `index_signature`, `property_signature` and `function_type` are type-level and would
/// otherwise flood a report with zeros.
const UNIT: &[&str] = &[
    "function_declaration",
    "generator_function_declaration",
    "function_expression",
    "generator_function",
    "arrow_function",
    "method_definition",
    "class_static_block",
];

/// Wrappers that do not change what a function is bound to.
const TRANSPARENT: &[&str] = &[
    "parenthesized_expression",
    "as_expression",
    "satisfies_expression",
    "non_null_expression",
    "type_assertion",
];

fn compiled_typescript() -> &'static Language {
    bonsai_core::compiled_once!({
        SPEC.compile(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap_or_else(|errors| {
                panic!("TypeScript spec does not match the linked grammar: {errors}")
            })
    })
}

fn compiled_tsx() -> &'static Language {
    bonsai_core::compiled_once!({
        SPEC.compile(tree_sitter_typescript::LANGUAGE_TSX.into())
            .unwrap_or_else(|errors| panic!("TSX spec does not match the linked grammar: {errors}"))
    })
}

/// `??` is deliberately absent, matching the specification's treatment of null-coalescing as
/// shorthand. The reference JavaScript analyser scores it, which is a documented divergence.
/// `in` and `instanceof` are comparisons, not flow breaks.
fn normalize_logical_operator(operator: &str) -> Option<&'static str> {
    match operator {
        "&&" => Some("&&"),
        "||" => Some("||"),
        _ => None,
    }
}

/// `break outer;` is the specification's labelled break; a plain `break` reads linearly.
fn is_penalized_jump(node: Node<'_>, _src: &[u8]) -> bool {
    node.child_by_field_name("label").is_some()
}

fn resolve_callee(node: Node<'_>) -> Option<Callee<'_>> {
    let callee = node.child_by_field_name("function")?;
    match callee.kind() {
        "identifier" => Some(Callee {
            receiver: None,
            name: callee,
        }),
        "member_expression" => Some(Callee {
            receiver: callee.child_by_field_name("object"),
            name: callee.child_by_field_name("property")?,
        }),
        _ => None,
    }
}

fn is_self_receiver(text: &str, container: Option<&str>) -> bool {
    matches!(text, "this" | "super")
        || container.is_some_and(|path| path.rsplit("::").next() == Some(text))
}

fn unit_name(node: Node<'_>, src: &[u8]) -> UnitName {
    if let Some(name) = node
        .child_by_field_name("name")
        .and_then(|name| text(name, src))
    {
        return UnitName::declared(strip_quotes(&name));
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
    if let Some(name) = node
        .child_by_field_name("name")
        .and_then(|name| text(name, src))
    {
        return Some(strip_quotes(&name));
    }
    bound_name(node, src)
}

/// Climbs to the declaration a marker would sit above. For `const handler = () => {}` the
/// comment precedes the whole declaration, not the arrow function; a class field or object pair
/// is the declaration for the arrow it holds.
fn suppression_anchor(node: Node<'_>) -> Node<'_> {
    let mut current = node;
    loop {
        let Some(parent) = current.parent() else {
            return current;
        };
        match parent.kind() {
            "variable_declarator"
            | "lexical_declaration"
            | "variable_declaration"
            | "export_statement"
            | "expression_statement"
            | "public_field_definition"
            | "pair" => current = parent,
            kind if TRANSPARENT.contains(&kind) => current = parent,
            _ => return current,
        }
    }
}

/// Walks up to whatever the function is bound to, stepping over wrappers that do not change the
/// binding. A binder only names the function when the function really is its value, so
/// `const x = cond ? f : g` does not claim the name for either branch.
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
                let name = parent.child_by_field_name("name")?;
                return (name.kind() == "identifier").then(|| text(name, src))?;
            }
            "pair" if is_value("value") => {
                return text(parent.child_by_field_name("key")?, src).map(|key| strip_quotes(&key));
            }
            "public_field_definition" if is_value("value") => {
                return text(parent.child_by_field_name("name")?, src).map(|n| strip_quotes(&n));
            }
            "assignment_expression" if is_value("right") => {
                return text(parent.child_by_field_name("left")?, src).map(|left| compact(&left));
            }
            "export_statement" if is_value("value") => return Some("default".to_string()),
            // A factory or wrapper call takes the name of whatever the call itself is bound to:
            // `const useCart = defineStore('cart', () => {})` is `useCart`, not `defineStore#1`.
            // Only a lone callable argument is unwrapped, so `app.get('/x', fn)` — where the
            // call is nobody's value — still falls through to a positional key.
            "arguments" if is_sole_callable_argument(parent, current) => {
                current = parent
                    .parent()
                    .filter(|call| call.kind() == "call_expression")?;
            }
            kind if TRANSPARENT.contains(&kind) => current = parent,
            _ => return None,
        }
    }
}

fn is_sole_callable_argument(arguments: Node<'_>, candidate: Node<'_>) -> bool {
    let mut cursor = arguments.walk();
    let mut callables = arguments
        .named_children(&mut cursor)
        .filter(|argument| UNIT.contains(&argument.kind()) || argument.kind() == "call_expression");

    callables
        .next()
        .is_some_and(|first| first.id() == candidate.id())
        && callables.next().is_none()
}

/// A callback that is nobody's value still needs a stable key, so it takes the call it belongs
/// to plus its argument position: `app.get#1`, `describe#1`.
fn positional_name(node: Node<'_>, src: &[u8]) -> Option<String> {
    let arguments = node.parent()?;
    if arguments.kind() != "arguments" {
        return None;
    }

    let call = arguments.parent()?;
    if call.kind() != "call_expression" {
        return None;
    }

    // An IIFE's callee is a parenthesised function, whose text is the whole body. Only a plain
    // name or member chain makes a key worth having; anything else stays anonymous.
    let function = call.child_by_field_name("function")?;
    if !matches!(function.kind(), "identifier" | "member_expression") {
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
