mod sfc;

use std::sync::OnceLock;

use bonsai_core::{Language, LanguageDescriptor, LanguageSpec};

/// Only the script blocks score. A `v-if` chain is branching, but the specification was not
/// written against templates and counting them would make a component incomparable with the same
/// logic written in TypeScript.
pub static VUE: LanguageDescriptor = LanguageDescriptor {
    id: "vue",
    extensions: &["vue"],
    spec: &VUE_SPEC,
    compiled: compiled_tsx,
    extract: Some(sfc::extract),
};

pub static VUE_SPEC: LanguageSpec = bonsai_lang_ts::spec("vue");

/// The kind-id table is resolved per grammar, so these cannot be shared with `bonsai-lang-ts`'s
/// even though the spec behind them is the same.
pub(crate) fn compiled_typescript() -> &'static Language {
    static COMPILED: OnceLock<Language> = OnceLock::new();
    COMPILED.get_or_init(|| {
        VUE_SPEC
            .compile(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap_or_else(|errors| {
                panic!("Vue spec does not match the linked TypeScript grammar: {errors}")
            })
    })
}

pub(crate) fn compiled_tsx() -> &'static Language {
    static COMPILED: OnceLock<Language> = OnceLock::new();
    COMPILED.get_or_init(|| {
        VUE_SPEC
            .compile(tree_sitter_typescript::LANGUAGE_TSX.into())
            .unwrap_or_else(|errors| {
                panic!("Vue spec does not match the linked TSX grammar: {errors}")
            })
    })
}
