//! A [`LanguageSpec`] compiled against its grammar, every kind and field resolved to an id.

use std::fmt;
use std::num::NonZeroU16;

use tree_sitter::{Language as TsLanguage, Node};

use crate::spec::LanguageSpec;

/// Mutually exclusive dispatch roles. A kind carries at most one, so the walker's hot path is a
/// single array index rather than a chain of string comparisons.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum Role {
    /// A kind no list claims, which the walker only descends through.
    #[default]
    Other,
    /// An `if` statement, costing +1 plus nesting, or a flat +1 where it continues an `else if`.
    If,
    /// An else-if clause with a node of its own, such as `elif`, costing a flat +1.
    ElseIf,
    /// An `else`, costing +1; in an if chain its body also nests a level deeper.
    Else,
    /// A loop, `switch`, `catch` or ternary, costing +1 plus nesting, its body a level deeper.
    Control,
    /// A `break` or `continue`, costing +1 only when the language's hooks penalise it.
    Jump,
    /// A jump that always costs +1, such as `goto`.
    Goto,
    /// A binary operation, whose boolean operators cost +1 per run of like ones.
    Logical,
    /// A call, costing +1 when it is direct recursion into the unit being scored.
    Call,
    /// A comment, the only place a suppression marker is read from.
    Trivia,
}

/// Properties that are not mutually exclusive with a role, nor with each other. One kind can be
/// both a unit and a nesting function, which is why these cannot be folded into `Role`.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Flags(u8);

impl Flags {
    /// A function-like kind, scored as a unit of its own unless another unit encloses it.
    pub const UNIT: Self = Self(1 << 0);
    /// A class-like kind whose name, where it has one, prefixes the key of each unit inside it.
    pub const CONTAINER: Self = Self(1 << 1);
    /// A closure-like kind, which costs nothing itself but nests its contents a level deeper.
    pub const NESTING_FN: Self = Self(1 << 2);
    /// A kind that leads a declaration, such as a decorator, stepped over to find its line and
    /// its marker.
    pub const LEADING_TRIVIA: Self = Self(1 << 3);
    /// A parenthesised expression, looked through so that grouping never ends an operator run.
    pub const PARENTHESIS: Self = Self(1 << 4);
    /// A kind that may open a file before its code, such as a shebang or a package clause, which
    /// the search for the file's marker steps over.
    pub const PREAMBLE: Self = Self(1 << 5);

    /// Returns whether the flag `other` is set.
    #[must_use]
    pub const fn has(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    #[must_use]
    const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// The role and flags one node kind carries, as a compiled [`Language`] stores them.
#[derive(Clone, Copy, Default, Debug)]
pub struct KindInfo {
    /// The kind's dispatch role, [`Role::Other`] when no list claims it.
    pub role: Role,
    /// The properties the kind carries besides its role.
    pub flags: Flags,
}

/// The field ids a [`Language`] reads, resolved from its spec's field names.
///
/// Each is `Some` once compilation succeeds, except `else_body` when the spec names none.
#[derive(Debug, Default)]
pub struct FieldIds {
    /// The field holding a declaration's name.
    pub name: Option<NonZeroU16>,
    /// The field holding a unit's or control structure's body, and an else-if clause's branch.
    pub body: Option<NonZeroU16>,
    /// The field holding the condition of an `if` or else-if.
    pub condition: Option<NonZeroU16>,
    /// The field holding the branch an `if` takes when its condition holds.
    pub if_then: Option<NonZeroU16>,
    /// The field holding an `if`'s else-if and else clauses, which may repeat.
    pub if_alternative: Option<NonZeroU16>,
    /// The field holding an else clause's body; without it, the clause's first named child that
    /// is not a comment is taken.
    pub else_body: Option<NonZeroU16>,
    /// The field holding a binary operation's left operand.
    pub logical_left: Option<NonZeroU16>,
    /// The field holding a binary operation's right operand.
    pub logical_right: Option<NonZeroU16>,
    /// The field holding a binary operation's operator, whose text the hooks normalise.
    pub logical_operator: Option<NonZeroU16>,
}

/// A [`LanguageSpec`] compiled against a tree-sitter grammar, its kinds and fields resolved to ids.
///
/// Built by [`LanguageSpec::compile`], usually once per process through
/// [`compiled_once!`](crate::compiled_once).
#[derive(Debug)]
pub struct Language {
    /// The spec this was compiled from.
    pub spec: &'static LanguageSpec,
    /// The grammar the spec was compiled against, which a parser must use for the ids to match.
    pub ts: TsLanguage,
    /// The spec's field names, resolved to this grammar's ids.
    pub fields: FieldIds,
    kinds: Box<[KindInfo]>,
    header_field: Box<[bool]>,
}

impl Language {
    /// `ERROR` and `_ERROR` sit at the top of the `u16` range, outside the table, so an
    /// out-of-range id is a real possibility rather than a defensive flourish.
    #[inline]
    #[must_use]
    pub fn info(&self, kind_id: u16) -> KindInfo {
        self.kinds
            .get(kind_id as usize)
            .copied()
            .unwrap_or_default()
    }

    /// Returns the role of `node`'s kind.
    #[inline]
    #[must_use]
    pub fn role(&self, node: Node<'_>) -> Role {
        self.info(node.kind_id()).role
    }

    /// Returns the flags of `node`'s kind.
    #[inline]
    #[must_use]
    pub fn flags(&self, node: Node<'_>) -> Flags {
        self.info(node.kind_id()).flags
    }

    /// Fields of a control structure that carry its header rather than its branches, and so do
    /// not raise the nesting level.
    #[inline]
    #[must_use]
    pub fn is_header_field(&self, field: Option<NonZeroU16>) -> bool {
        field.is_some_and(|id| {
            self.header_field
                .get(id.get() as usize)
                .copied()
                .unwrap_or(false)
        })
    }
}

/// Returns the first child of `node` in the field `id`, finding nothing when `id` is `None`.
#[inline]
#[must_use]
pub fn field(node: Node<'_>, id: Option<NonZeroU16>) -> Option<Node<'_>> {
    id.and_then(|id| node.child_by_field_id(id.get()))
}

/// Every mismatch found between a [`LanguageSpec`] and the grammar it was compiled against.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SpecErrors {
    /// Declared kinds, other than optional ones, that the grammar has no named node for, sorted.
    pub unknown_kinds: Vec<&'static str>,
    /// Declared field names the grammar lacks, sorted.
    pub unknown_fields: Vec<&'static str>,
    /// Kinds listed under two roles, each with the role it had and the one that followed.
    pub conflicting_roles: Vec<(&'static str, Role, Role)>,
}

impl SpecErrors {
    /// Returns whether no mismatch was found.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.unknown_kinds.is_empty()
            && self.unknown_fields.is_empty()
            && self.conflicting_roles.is_empty()
    }
}

impl fmt::Display for SpecErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.unknown_kinds.is_empty() {
            write!(
                f,
                "node kinds absent from the grammar: {:?}. ",
                self.unknown_kinds
            )?;
        }
        if !self.unknown_fields.is_empty() {
            write!(
                f,
                "field names absent from the grammar: {:?}. ",
                self.unknown_fields
            )?;
        }
        for (kind, first, second) in &self.conflicting_roles {
            write!(f, "kind {kind} claimed by both {first:?} and {second:?}. ")?;
        }
        Ok(())
    }
}

impl std::error::Error for SpecErrors {}

impl LanguageSpec {
    fn note_unknown_kind(&self, kind: &'static str, errors: &mut SpecErrors) {
        if !self.optional_kinds.contains(&kind) {
            errors.unknown_kinds.push(kind);
        }
    }

    /// Compiles the spec against a grammar, resolving each declared kind and field name to an id.
    ///
    /// Fails naming every kind and field the grammar lacks and every kind claimed by two roles, so
    /// a grammar upgrade that renames a node is caught rather than scored as zero.
    pub fn compile(&'static self, ts: TsLanguage) -> Result<Language, SpecErrors> {
        let mut errors = SpecErrors::default();
        let mut kinds = vec![KindInfo::default(); ts.node_kind_count()];
        self.assign_roles(&ts, &mut kinds, &mut errors);
        self.assign_flags(&ts, &mut kinds, &mut errors);
        let header_field = self.header_fields(&ts, &mut errors);

        let f = &self.fields;
        let fields = FieldIds {
            name: required_field(&ts, f.name, &mut errors),
            body: required_field(&ts, f.body, &mut errors),
            condition: required_field(&ts, f.condition, &mut errors),
            if_then: required_field(&ts, f.if_then, &mut errors),
            if_alternative: required_field(&ts, f.if_alternative, &mut errors),
            else_body: f
                .else_body
                .and_then(|n| required_field(&ts, n, &mut errors)),
            logical_left: required_field(&ts, f.logical_left, &mut errors),
            logical_right: required_field(&ts, f.logical_right, &mut errors),
            logical_operator: required_field(&ts, f.logical_operator, &mut errors),
        };

        errors.unknown_kinds.sort_unstable();
        errors.unknown_kinds.dedup();
        errors.unknown_fields.sort_unstable();
        errors.unknown_fields.dedup();

        if !errors.is_empty() {
            return Err(errors);
        }

        Ok(Language {
            spec: self,
            ts,
            fields,
            kinds: kinds.into_boxed_slice(),
            header_field: header_field.into_boxed_slice(),
        })
    }

    fn assign_roles(
        &'static self,
        ts: &TsLanguage,
        kinds: &mut [KindInfo],
        errors: &mut SpecErrors,
    ) {
        let roles: [(&'static [&'static str], Role); 9] = [
            (self.kinds.if_statement, Role::If),
            (self.kinds.else_if_clause, Role::ElseIf),
            (self.kinds.else_clause, Role::Else),
            (self.kinds.nesting_control, Role::Control),
            (self.kinds.jump, Role::Jump),
            (self.kinds.unconditional_jump, Role::Goto),
            (self.kinds.logical, Role::Logical),
            (self.kinds.call, Role::Call),
            (self.kinds.comment, Role::Trivia),
        ];

        for (kind, role) in each_kind(&roles) {
            match named_kind_id(ts, kind, kinds.len()) {
                Some(id) => claim_role(&mut kinds[id], kind, role, errors),
                None => self.note_unknown_kind(kind, errors),
            }
        }
    }

    fn assign_flags(
        &'static self,
        ts: &TsLanguage,
        kinds: &mut [KindInfo],
        errors: &mut SpecErrors,
    ) {
        let flags: [(&'static [&'static str], Flags); 6] = [
            (self.kinds.unit, Flags::UNIT),
            (self.kinds.container, Flags::CONTAINER),
            (self.kinds.nesting_function, Flags::NESTING_FN),
            (self.kinds.leading_trivia, Flags::LEADING_TRIVIA),
            (self.kinds.parenthesis, Flags::PARENTHESIS),
            (self.kinds.preamble, Flags::PREAMBLE),
        ];

        for (kind, flag) in each_kind(&flags) {
            match named_kind_id(ts, kind, kinds.len()) {
                Some(id) => kinds[id].flags = kinds[id].flags.with(flag),
                None => self.note_unknown_kind(kind, errors),
            }
        }
    }

    /// Field ids run from 1 to `field_count`, so the table has one slot per id.
    fn header_fields(&self, ts: &TsLanguage, errors: &mut SpecErrors) -> Vec<bool> {
        let mut header_field = vec![false; ts.field_count() + 1];
        for name in self.fields.control_header {
            match ts.field_id_for_name(name) {
                Some(id) => header_field[id.get() as usize] = true,
                None => errors.unknown_fields.push(name),
            }
        }
        header_field
    }
}

fn each_kind<'a, T: Copy + 'a>(
    lists: &'a [(&'static [&'static str], T)],
) -> impl Iterator<Item = (&'static str, T)> + 'a {
    lists
        .iter()
        .flat_map(|(list, value)| list.iter().map(move |kind| (*kind, *value)))
}

fn claim_role(slot: &mut KindInfo, kind: &'static str, role: Role, errors: &mut SpecErrors) {
    if slot.role != Role::Other && slot.role != role {
        errors.conflicting_roles.push((kind, slot.role, role));
    }
    slot.role = role;
}

/// Symbol 0 is tree-sitter's end-of-input marker, so it doubles as the "no such kind" answer
/// from `id_for_node_kind`.
fn named_kind_id(ts: &TsLanguage, kind: &str, kind_count: usize) -> Option<usize> {
    let id = ts.id_for_node_kind(kind, true) as usize;
    if id == 0 || id >= kind_count {
        None
    } else {
        Some(id)
    }
}

fn required_field(
    ts: &TsLanguage,
    name: &'static str,
    errors: &mut SpecErrors,
) -> Option<NonZeroU16> {
    let id = ts.field_id_for_name(name);
    if id.is_none() {
        errors.unknown_fields.push(name);
    }
    id
}

/// Memoises a compiled [`Language`] as a `&'static`.
///
/// Split per target because tree-sitter gates `unsafe impl Sync for Language` off for wasm, so
/// a `static OnceLock<Language>` will not compile there. wasm32 is single-threaded, so a
/// thread-local plus a one-time leak is equivalent and still needs no unsafe.
#[cfg(not(target_family = "wasm"))]
#[macro_export]
macro_rules! compiled_once {
    ($build:expr) => {{
        static COMPILED: ::std::sync::OnceLock<$crate::Language> = ::std::sync::OnceLock::new();
        COMPILED.get_or_init(|| $build)
    }};
}

#[cfg(target_family = "wasm")]
#[macro_export]
macro_rules! compiled_once {
    ($build:expr) => {{
        thread_local! {
            static COMPILED: ::std::cell::OnceCell<&'static $crate::Language> =
                const { ::std::cell::OnceCell::new() };
        }
        COMPILED.with(|cell| {
            *cell.get_or_init(|| ::std::boxed::Box::leak(::std::boxed::Box::new($build)))
        })
    }};
}
