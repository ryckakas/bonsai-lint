mod collect;
pub mod finding;
pub mod language;
pub mod naming;
pub mod spec;
pub mod suppression;
pub mod walk;

pub use collect::{analyze, declaration_row, disambiguate};
pub use finding::{
    Callee, Finding, NameOrigin, Suppression, UnitName, ANONYMOUS_UNIT, SUPPRESSION_MARKER,
    TOPLEVEL_UNIT,
};
pub use language::{Flags, KindInfo, Language, Role, SpecErrors};
pub use spec::{FieldNames, Hooks, KindSets, LanguageSpec};

/// What the registry stores for a language. Compilation is memoised behind
/// [`compiled_once!`], so the facade never names a grammar crate's types.
#[derive(Debug)]
pub struct LanguageDescriptor {
    pub id: &'static str,
    pub extensions: &'static [&'static str],
    pub spec: &'static LanguageSpec,
    pub compiled: fn() -> &'static Language,
    /// `None` parses the whole file with `compiled`.
    pub extract: Option<fn(&str) -> Extraction>,
}

/// Ranges are parsed against the whole file rather than an extracted substring, so node positions
/// are already positions in the original file and no line offset is ever applied.
#[derive(Debug)]
pub struct Extraction {
    pub language: &'static Language,
    /// Ordered and non-overlapping, as `set_included_ranges` requires. Empty scores nothing.
    pub ranges: Vec<tree_sitter::Range>,
    /// A host grammar has no spec to fail compilation, so an upgrade that stopped finding blocks
    /// would otherwise report zero findings in silence.
    pub warning: Option<String>,
}
