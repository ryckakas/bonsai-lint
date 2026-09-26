use std::borrow::Cow;

use tree_sitter::Node;

pub const SUPPRESSION_MARKER: &str = "bonsai-lint-ignore";
pub const TOPLEVEL_UNIT: &str = "<toplevel>";
pub const ANONYMOUS_UNIT: &str = "<anonymous>";

/// A suppression is only honoured when it carries a reason. A bare marker is reported as
/// `MissingReason` rather than silently obeyed, so that silencing a finding stays a documented
/// decision rather than a reflex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Suppression {
    None,
    Reasoned(String),
    MissingReason,
}

/// Where a unit's name came from. Only `Positional` and `Anonymous` names are unstable enough
/// to need a collision suffix; `Declared` and `Bound` are pure syntax at the binding site and
/// survive reordering, reformatting and body edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameOrigin {
    Declared,
    Bound,
    Positional,
    Anonymous,
    TopLevel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitName {
    pub text: String,
    pub origin: NameOrigin,
    /// Joined onto `text` in the key, but never matched against a call: overloads and property
    /// accessors share one callable name and differ only here.
    pub signature: Option<String>,
}

impl UnitName {
    #[must_use]
    pub fn declared(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            origin: NameOrigin::Declared,
            signature: None,
        }
    }

    #[must_use]
    pub fn bound(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            origin: NameOrigin::Bound,
            signature: None,
        }
    }

    #[must_use]
    pub fn positional(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            origin: NameOrigin::Positional,
            signature: None,
        }
    }

    #[must_use]
    pub fn anonymous() -> Self {
        Self {
            text: ANONYMOUS_UNIT.to_string(),
            origin: NameOrigin::Anonymous,
            signature: None,
        }
    }

    #[must_use]
    pub fn with_signature(self, signature: impl Into<String>) -> Self {
        Self {
            signature: Some(signature.into()),
            ..self
        }
    }
}

/// How a call reaches the unit it is in, read once from the unit's declaration. A call is
/// recursion when it names the unit and goes through one of `self_receivers`, or through no
/// receiver at all where `bare_call_recurses` allows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnitScope {
    /// A path segment the declaration names itself, for a method written outside its type's body.
    pub container: Option<String>,
    pub self_receivers: Vec<Cow<'static, str>>,
    pub bare_call_recurses: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub container: Option<String>,
    pub name: String,
    pub origin: NameOrigin,
    pub line: usize,
    pub score: u32,
    pub suppression: Suppression,
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

    #[must_use]
    pub fn is_suppressed(&self) -> bool {
        matches!(self.suppression, Suppression::Reasoned(_))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Callee<'t> {
    pub receiver: Option<Node<'t>>,
    pub name: Node<'t>,
}
