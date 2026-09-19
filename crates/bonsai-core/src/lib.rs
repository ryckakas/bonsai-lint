mod collect;
pub mod finding;
pub mod language;
pub mod naming;
pub mod spec;
pub mod suppression;
pub mod walk;

pub use collect::{analyze, declaration_row};
pub use finding::{
    Callee, Finding, NameOrigin, Suppression, UnitName, ANONYMOUS_UNIT, SUPPRESSION_MARKER,
    TOPLEVEL_UNIT,
};
pub use language::{Flags, KindInfo, Language, Role, SpecErrors};
pub use spec::{FieldNames, Hooks, KindSets, LanguageSpec};

/// What the registry stores for a language. Compilation is memoised behind each language
/// crate's own `OnceLock`, so the facade never names a grammar crate's types.
#[derive(Debug)]
pub struct LanguageDescriptor {
    pub id: &'static str,
    pub extensions: &'static [&'static str],
    pub spec: &'static LanguageSpec,
    pub compiled: fn() -> &'static Language,
}
