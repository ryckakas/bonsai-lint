mod collect;
pub mod finding;
pub mod language;
pub mod naming;
pub mod spec;
pub mod suppression;
pub mod walk;

pub use collect::{analyze, declaration_row, disambiguate};
pub use finding::{
    Callee, Finding, NameOrigin, Suppression, UnitName, UnitScope, ANONYMOUS_UNIT,
    SUPPRESSION_MARKER, TOPLEVEL_UNIT,
};
pub use language::{Flags, KindInfo, Language, Role, SpecErrors};
pub use spec::{FieldNames, Hooks, KindSets, LanguageSpec};
pub use walk::IfPart;

/// What the registry stores for a file type. Compilation is memoised behind
/// [`compiled_once!`], so the facade never names a grammar crate's types.
pub trait LanguageDescriptor: Sync + std::fmt::Debug {
    fn spec(&self) -> &'static LanguageSpec;

    fn extensions(&self) -> &'static [&'static str];

    /// Each implementor returns its own `compiled_once!` static: one shared body would be a
    /// single static, handing every descriptor whichever grammar compiled first.
    fn compiled(&self) -> &'static Language;

    /// `None` parses the whole file with `compiled`.
    fn extract(&self, _source: &str) -> Option<Extraction> {
        None
    }

    /// A language's own convention for marking machine-written files, which are neither scored
    /// nor counted: nobody refactors them, so a finding there is noise.
    fn is_generated(&self, _source: &str) -> bool {
        false
    }

    /// File names that carry no code worth scoring despite their extension: signatures only, or
    /// minified output that is one unit nobody will refactor. Whole suffixes, not substrings, so
    /// a name that merely contains one still scores.
    fn unscored_suffixes(&self) -> &'static [&'static str] {
        &[]
    }
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
