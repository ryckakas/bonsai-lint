//! Findings, and the names, scopes and callees that hooks hand the scorer to build them.

use std::borrow::Cow;

use tree_sitter::Node;

/// The text that marks a comment as a suppression, honoured only when `:` and a reason follow.
pub const SUPPRESSION_MARKER: &str = "bonsai-lint-ignore";
/// The name of the unit holding a file's code outside any function-like.
pub const TOPLEVEL_UNIT: &str = "<toplevel>";
/// The name of a unit with no declared, bound or positional name.
pub const ANONYMOUS_UNIT: &str = "<anonymous>";

/// Whether a suppression marker applies, and whether it gave a reason.
///
/// A suppression is only honoured when it carries a reason. A bare marker is reported as
/// `MissingReason` rather than silently obeyed, so that silencing a finding stays a documented
/// decision rather than a reflex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Suppression {
    /// No marker covers the unit.
    None,
    /// A marker and the reason written after it; the finding is suppressed.
    Reasoned(String),
    /// A marker without a reason, which does not suppress the finding.
    MissingReason,
}

/// Where a unit's name came from.
///
/// Only `Positional` and `Anonymous` names are unstable enough
/// to need a collision suffix; `Declared` and `Bound` are pure syntax at the binding site and
/// survive reordering, reformatting and body edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameOrigin {
    /// Read from the unit's own declaration, as in `function parse() {}`.
    Declared,
    /// Taken from what the unit is bound to, as in `const handler = () => {}`.
    Bound,
    /// Built from where the unit sits, as in a call argument's `app.get#1`, or a declared name the
    /// language lets repeat, such as Go's `init`.
    Positional,
    /// Nothing names the unit, so its name is [`ANONYMOUS_UNIT`].
    Anonymous,
    /// The file's code outside any unit, named [`TOPLEVEL_UNIT`].
    TopLevel,
}

/// The name a unit is keyed by, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitName {
    /// The name, as matched against calls when counting recursion.
    pub text: String,
    /// Where [`text`](Self::text) came from.
    pub origin: NameOrigin,
    /// Joined onto `text` in the key, but never matched against a call: overloads and property
    /// accessors share one callable name and differ only here.
    pub signature: Option<String>,
}

impl UnitName {
    /// Builds a name read from the unit's own declaration.
    #[must_use]
    pub fn declared(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            origin: NameOrigin::Declared,
            signature: None,
        }
    }

    /// Builds a name taken from what the unit is bound to, such as the variable holding a closure.
    #[must_use]
    pub fn bound(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            origin: NameOrigin::Bound,
            signature: None,
        }
    }

    /// Builds a name from where the unit sits, which [`disambiguate`](crate::disambiguate)
    /// suffixes on a collision.
    #[must_use]
    pub fn positional(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            origin: NameOrigin::Positional,
            signature: None,
        }
    }

    /// Builds the [`ANONYMOUS_UNIT`] name for a unit nothing names.
    #[must_use]
    pub fn anonymous() -> Self {
        Self {
            text: ANONYMOUS_UNIT.to_string(),
            origin: NameOrigin::Anonymous,
            signature: None,
        }
    }

    /// Returns the name with a signature added, to tell apart units that share a callable name.
    #[must_use]
    pub fn with_signature(self, signature: impl Into<String>) -> Self {
        Self {
            signature: Some(signature.into()),
            ..self
        }
    }
}

/// A unit's own scope: the receivers through which a call reaches it, and any container its
/// declaration names.
///
/// How a call reaches the unit it is in, read once from the unit's declaration. A call is
/// recursion when it names the unit and goes through one of `self_receivers`, or through no
/// receiver at all where `bare_call_recurses` allows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnitScope {
    /// A path segment the declaration names itself, for a method written outside its type's body.
    pub container: Option<String>,
    /// Receivers through which a call of the unit's name reaches it, matched as source text.
    pub self_receivers: Vec<Cow<'static, str>>,
    /// Whether a call of the unit's name with no receiver reaches it.
    pub bare_call_recurses: bool,
}

/// A scored unit: a function-like, or a file's [`TOPLEVEL_UNIT`] code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The `::`-joined path of the containers the unit belongs to, `None` if there are none.
    pub container: Option<String>,
    /// The unit's name as keyed, signature included, with a `~2`, `~3`, … suffix on a collision.
    pub name: String,
    /// Where the name came from.
    pub origin: NameOrigin,
    /// The 1-based line the unit is reported on: its signature, below any attributes or decorators.
    ///
    /// [`TOPLEVEL_UNIT`] is reported where its parsed region starts, line 1 unless the file embeds
    /// its code in a host syntax.
    pub line: usize,
    /// The unit's cognitive complexity, nested function-likes included.
    pub score: u32,
    /// Whether a [`SUPPRESSION_MARKER`] comment covers the unit, and with what reason.
    pub suppression: Suppression,
    /// The [`id`](crate::LanguageSpec::id) of the spec that scored the unit.
    pub language: &'static str,
}

impl Finding {
    /// `Class::method`, or the bare name for a free function. This is the identity a baseline
    /// records, so it deliberately excludes the line number — otherwise every edit above a
    /// function would invalidate it.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        match &self.container {
            Some(container) => format!("{container}::{}", self.name),
            None => self.name.clone(),
        }
    }

    /// Returns whether a marker with a reason suppresses this finding.
    #[must_use]
    pub fn is_suppressed(&self) -> bool {
        matches!(self.suppression, Suppression::Reasoned(_))
    }
}

/// A call's receiver and called name, as the recursion check reads them.
#[derive(Debug, Clone, Copy)]
pub struct Callee<'t> {
    /// The expression the call goes through, `None` for a bare call.
    pub receiver: Option<Node<'t>>,
    /// The node naming what is called, compared as text with the unit's name.
    pub name: Node<'t>,
}
