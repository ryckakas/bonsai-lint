use std::fmt::Debug;

use tree_sitter::Node;

use crate::finding::{Callee, UnitName, UnitReceiver};

#[derive(Debug)]
pub struct LanguageSpec {
    pub id: &'static str,
    pub kinds: KindSets,
    pub fields: FieldNames,
    pub hooks: &'static dyn Hooks,
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
    /// What may precede code at the top of a file: an open tag, a shebang. A file-level marker
    /// is searched for behind these.
    pub preamble: &'static [&'static str],
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
            self.preamble,
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

/// A method is required when every language must answer it, and defaulted only when the default
/// is right for a language without the feature. A method added later ships with a default that
/// keeps today's behaviour, so no existing language has to change.
pub trait Hooks: Sync + Debug {
    fn normalize_logical_operator(&self, operator: &str) -> Option<&'static str>;

    fn is_penalized_jump(&self, node: Node<'_>, src: &[u8]) -> bool;

    fn resolve_callee<'t>(&self, node: Node<'t>) -> Option<Callee<'t>>;

    fn is_self_receiver(&self, text: &str, container: Option<&str>) -> bool;

    fn unit_name(&self, node: Node<'_>, src: &[u8]) -> UnitName;

    fn unit_receiver(&self, _node: Node<'_>, _src: &[u8]) -> Option<UnitReceiver> {
        None
    }

    fn container_name(&self, _node: Node<'_>, _src: &[u8]) -> Option<String> {
        None
    }

    fn suppression_anchor<'t>(&self, node: Node<'t>) -> Node<'t> {
        node
    }
}
