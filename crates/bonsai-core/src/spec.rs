use std::fmt::Debug;

use tree_sitter::Node;

use crate::finding::{Callee, UnitName, UnitScope};
use crate::language::{field, Language};
use crate::walk::IfPart;

#[derive(Debug)]
pub struct LanguageSpec {
    pub id: &'static str,
    pub kinds: KindSets,
    pub fields: FieldNames,
    pub hooks: &'static dyn Hooks,
    /// Kinds a grammar is allowed not to have. When a grammar renames a kind between releases,
    /// both spellings are declared and whichever is live wins, rather than pinning users to one
    /// grammar version.
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
///
/// The smallest language both traits accept. It implements only what is required, so adding a
/// required method breaks this example on purpose.
///
/// ```no_run
/// use bonsai_core::{
///     Callee, Hooks, Language, LanguageDescriptor, LanguageSpec, UnitName, UnitScope,
/// };
/// use tree_sitter::Node;
///
/// #[derive(Debug)]
/// struct Minimal;
///
/// impl Hooks for Minimal {
///     fn normalize_logical_operator(&self, _: &str) -> Option<&'static str> {
///         None
///     }
///
///     fn is_penalized_jump(&self, _: Node<'_>, _: &[u8]) -> bool {
///         false
///     }
///
///     fn resolve_callee<'t>(&self, _: Node<'t>) -> Option<Callee<'t>> {
///         None
///     }
///
///     fn unit_name(&self, _: Node<'_>, _: &[u8]) -> UnitName {
///         UnitName::anonymous()
///     }
///
///     fn unit_scope(&self, _: Node<'_>, _: &[u8], _: Option<&str>) -> UnitScope {
///         UnitScope::default()
///     }
/// }
///
/// impl LanguageDescriptor for Minimal {
///     fn spec(&self) -> &'static LanguageSpec {
///         unimplemented!()
///     }
///
///     fn extensions(&self) -> &'static [&'static str] {
///         &[]
///     }
///
///     fn compiled(&self) -> &'static Language {
///         unimplemented!()
///     }
/// }
/// ```
pub trait Hooks: Sync + Debug {
    fn normalize_logical_operator(&self, operator: &str) -> Option<&'static str>;

    fn is_penalized_jump(&self, node: Node<'_>, src: &[u8]) -> bool;

    fn resolve_callee<'t>(&self, node: Node<'t>) -> Option<Callee<'t>>;

    fn unit_name(&self, node: Node<'_>, src: &[u8]) -> UnitName;

    /// `container` is the path of the containers enclosing the unit, already joined.
    fn unit_scope(&self, node: Node<'_>, src: &[u8], container: Option<&str>) -> UnitScope;

    fn container_name(&self, _node: Node<'_>, _src: &[u8]) -> Option<String> {
        None
    }

    /// Hands `visit` each node a unit's marker may lead or trail, innermost first; the first one
    /// carrying a marker wins.
    fn suppression_anchors<'t>(
        &self,
        node: Node<'t>,
        _src: &[u8],
        visit: &mut dyn FnMut(Node<'t>),
    ) {
        visit(node);
    }

    /// Hands `visit` the parts of an if or else-if node, for a grammar whose chains the field
    /// names alone cannot describe.
    fn if_parts<'t>(&self, node: Node<'t>, lang: &Language, visit: &mut dyn FnMut(IfPart<'t>)) {
        crate::walk::if_parts_by_fields(node, lang, visit);
    }

    /// The code a unit scores, for a unit kind whose grammar leaves its body unfielded. `None`
    /// is a bodyless declaration, which is not a unit.
    fn unit_body<'t>(&self, node: Node<'t>, lang: &Language) -> Option<Node<'t>> {
        field(node, lang.fields.body)
    }

    /// Asked only once a call already names the unit through one of its own receivers. A
    /// language with overloading answers whether this call site could reach this declaration.
    fn call_reaches_unit(&self, _call: Node<'_>, _unit: Node<'_>, _src: &[u8]) -> bool {
        true
    }
}
