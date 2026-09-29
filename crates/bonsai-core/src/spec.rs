//! The per-language description the scorer runs on: node kinds, field names and hooks.

use std::fmt::Debug;

use tree_sitter::Node;

use crate::finding::{Callee, UnitName, UnitScope};
use crate::language::{Language, field};
use crate::walk::IfPart;

/// A language's syntax as the scorer reads it: node kinds by role, field names and hooks.
///
/// [`compile`](Self::compile) resolves it against a grammar into the [`Language`] the scorer
/// indexes by kind id.
#[derive(Debug)]
pub struct LanguageSpec {
    /// The id every [`Finding`](crate::Finding) scored with this spec carries as its language.
    pub id: &'static str,
    /// The node kinds for each syntactic role.
    pub kinds: KindSets,
    /// The field names the walker reads a node's parts through.
    pub fields: FieldNames,
    /// The tree-reading that kinds and field names cannot express.
    pub hooks: &'static dyn Hooks,
    /// Kinds a grammar is allowed not to have. When a grammar renames a kind between releases,
    /// both spellings are declared and whichever is live wins, rather than pinning users to one
    /// grammar version.
    pub optional_kinds: &'static [&'static str],
}

/// Node kinds grouped by the part they play in scoring.
///
/// `unit`, `container`, `nesting_function`, `leading_trivia`, `parenthesis` and `preamble` may
/// share a kind with any list. The others are exclusive: a kind in two of them fails compilation.
#[derive(Debug)]
pub struct KindSets {
    /// Function-likes, each scored as a unit of its own unless another unit encloses it.
    pub unit: &'static [&'static str],
    /// Kinds whose name prefixes the keys of the units inside them, such as a class.
    pub container: &'static [&'static str],
    /// Kinds that cost nothing but raise the nesting level of what they hold, such as a closure.
    pub nesting_function: &'static [&'static str],
    /// `if` statements, each +1 plus the nesting level, or a flat +1 where one continues an
    /// `else if`.
    pub if_statement: &'static [&'static str],
    /// Else-if clauses with a node of their own, such as `elseif`, each a flat +1.
    pub else_if_clause: &'static [&'static str],
    /// Else clauses, whose branch costs a flat +1.
    pub else_clause: &'static [&'static str],
    /// Control structures other than `if`, such as loops and `catch`, each +1 plus nesting.
    pub nesting_control: &'static [&'static str],
    /// Jumps that cost +1 when [`Hooks::is_penalized_jump`] says so, such as a labelled `break`.
    pub jump: &'static [&'static str],
    /// Jumps that always cost +1, such as `goto`.
    pub unconditional_jump: &'static [&'static str],
    /// Binary expressions read for logical operators, where each run of like operators costs +1.
    pub logical: &'static [&'static str],
    /// Parenthesised expressions, which a run of logical operators continues through.
    pub parenthesis: &'static [&'static str],
    /// Calls, each checked for direct recursion, which costs +1.
    pub call: &'static [&'static str],
    /// Comments, which score nothing and may carry a suppression marker.
    pub comment: &'static [&'static str],
    /// Nodes such as decorators that a declaration's reported line and its marker search step over.
    pub leading_trivia: &'static [&'static str],
    /// What may precede code at the top of a file: an open tag, a shebang. A file-level marker
    /// is searched for behind these.
    pub preamble: &'static [&'static str],
}

impl KindSets {
    /// Returns every list's kinds in turn, without removing repeats.
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

/// The grammar's field names for the parts of a node the walker reads.
///
/// Every name must exist in the grammar, or compiling the spec fails.
#[derive(Debug)]
pub struct FieldNames {
    /// A declaration's name.
    pub name: &'static str,
    /// The body of a unit, a control structure or an else-if clause.
    pub body: &'static str,
    /// The condition of an `if` or else-if, scored at the chain's own depth.
    pub condition: &'static str,
    /// The branch an `if` runs when its condition holds.
    pub if_then: &'static str,
    /// The else-ifs and else that follow an `if`'s then branch.
    pub if_alternative: &'static str,
    /// An else clause's body; without one, the clause's first named child that is not a comment.
    pub else_body: Option<&'static str>,
    /// The left operand of a logical expression.
    pub logical_left: &'static str,
    /// The right operand of a logical expression.
    pub logical_right: &'static str,
    /// The operator of a logical expression, whose text [`Hooks::normalize_logical_operator`]
    /// reads.
    pub logical_operator: &'static str,
    /// Fields of a control structure that hold its header, scored at the structure's own depth.
    pub control_header: &'static [&'static str],
}

/// The tree-reading that differs between languages, called where kinds and field names are not
/// enough.
///
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
    /// Returns the operator a logical operator's source text counts as in a run.
    ///
    /// Spellings that map alike share a run, as `and` and `&&` can. `None` is any other operator,
    /// such as `??` or `+`, which costs nothing.
    fn normalize_logical_operator(&self, operator: &str) -> Option<&'static str>;

    /// Returns whether a [`jump`](KindSets::jump) costs +1, as a labelled `break` does.
    fn is_penalized_jump(&self, node: Node<'_>, src: &[u8]) -> bool;

    /// Reads the receiver and name a call goes through, for the recursion check.
    ///
    /// `None` is a call the check cannot read, which never counts as recursion.
    fn resolve_callee<'t>(&self, node: Node<'t>) -> Option<Callee<'t>>;

    /// Names a unit, recording where the name came from.
    fn unit_name(&self, node: Node<'_>, src: &[u8]) -> UnitName;

    /// `container` is the path of the containers enclosing the unit, already joined.
    fn unit_scope(&self, node: Node<'_>, src: &[u8], container: Option<&str>) -> UnitScope;

    /// Names a container, as the segment prefixed to the keys of the units inside it.
    ///
    /// `None` adds no segment, so those units keep the enclosing path.
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

    /// A header the grammar leaves without a field and without a body to stand before, such as
    /// the condition in the middle of Python's `a if c else b`.
    fn is_control_header(&self, _control: Node<'_>, _child: Node<'_>) -> bool {
        false
    }
}
