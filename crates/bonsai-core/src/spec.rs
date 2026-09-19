use tree_sitter::Node;

use crate::finding::{Callee, UnitName};

#[derive(Debug)]
pub struct LanguageSpec {
    pub id: &'static str,
    pub kinds: KindSets,
    pub fields: FieldNames,
    pub hooks: Hooks,
    /// Kinds a grammar is allowed not to have. The PHP grammar renamed anonymous functions
    /// between releases, so both spellings are declared and whichever is live wins, rather than
    /// pinning users to one grammar version.
    pub optional_kinds: &'static [&'static str],
}

#[derive(Debug)]
pub struct KindSets {
    pub unit: &'static [&'static str],
    pub container: &'static [&'static str],
    pub nesting_function: &'static [&'static str],
    pub if_statement: &'static [&'static str],
    pub else_if_clause: &'static [&'static str],
    pub else_clause: &'static [&'static str],
    pub nesting_control: &'static [&'static str],
    pub jump: &'static [&'static str],
    pub unconditional_jump: &'static [&'static str],
    pub logical: &'static [&'static str],
    pub parenthesis: &'static [&'static str],
    pub call: &'static [&'static str],
    pub comment: &'static [&'static str],
    pub leading_trivia: &'static [&'static str],
}

impl KindSets {
    pub fn all(&self) -> impl Iterator<Item = &'static str> + '_ {
        [
            self.unit,
            self.container,
            self.nesting_function,
            self.if_statement,
            self.else_if_clause,
            self.else_clause,
            self.nesting_control,
            self.jump,
            self.unconditional_jump,
            self.logical,
            self.parenthesis,
            self.call,
            self.comment,
            self.leading_trivia,
        ]
        .into_iter()
        .flatten()
        .copied()
    }
}

#[derive(Debug)]
pub struct FieldNames {
    pub name: &'static str,
    pub body: &'static str,
    pub condition: &'static str,
    pub if_then: &'static str,
    pub if_alternative: &'static str,
    pub else_body: Option<&'static str>,
    pub logical_left: &'static str,
    pub logical_right: &'static str,
    pub logical_operator: &'static str,
    pub control_header: &'static [&'static str],
}

#[derive(Debug)]
pub struct Hooks {
    pub normalize_logical_operator: fn(&str) -> Option<&'static str>,
    pub is_penalized_jump: for<'t> fn(Node<'t>, &[u8]) -> bool,
    pub resolve_callee: for<'t> fn(Node<'t>) -> Option<Callee<'t>>,
    pub is_self_receiver: fn(&str, Option<&str>) -> bool,
    pub unit_name: for<'t> fn(Node<'t>, &[u8]) -> UnitName,
    pub container_name: for<'t> fn(Node<'t>, &[u8]) -> Option<String>,
    pub suppression_anchor: for<'t> fn(Node<'t>) -> Node<'t>,
}
