use std::sync::OnceLock;

use bonsai_core::{
    Callee, FieldNames, Hooks, KindSets, Language, LanguageDescriptor, LanguageSpec, UnitName,
};
use tree_sitter::Node;

pub static PHP: LanguageDescriptor = LanguageDescriptor {
    id: "php",
    extensions: &["php", "phtml"],
    spec: &SPEC,
    compiled,
};

pub static SPEC: LanguageSpec = LanguageSpec {
    id: "php",
    kinds: KindSets {
        unit: &["function_definition", "method_declaration"],
        container: &[
            "class_declaration",
            "interface_declaration",
            "trait_declaration",
            "enum_declaration",
        ],
        nesting_function: &[
            "anonymous_function",
            "anonymous_function_creation_expression",
            "arrow_function",
            "function_definition",
        ],
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
        call: &[
            "function_call_expression",
            "member_call_expression",
            "scoped_call_expression",
        ],
        comment: &["comment"],
        leading_trivia: &["attribute_list"],
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
        control_header: &["condition"],
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
    optional_kinds: &["anonymous_function_creation_expression"],
};

fn compiled() -> &'static Language {
    static COMPILED: OnceLock<Language> = OnceLock::new();
    COMPILED.get_or_init(|| {
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
    node.child_by_field_name("name")
        .and_then(|name| name.utf8_text(src).ok())
        .map_or_else(UnitName::anonymous, UnitName::declared)
}

fn container_name(node: Node<'_>, src: &[u8]) -> Option<String> {
    node.child_by_field_name("name")?
        .utf8_text(src)
        .ok()
        .map(ToString::to_string)
}

fn suppression_anchor(node: Node<'_>) -> Node<'_> {
    node
}
