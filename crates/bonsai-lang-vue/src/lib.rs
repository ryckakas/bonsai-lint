mod sfc;

use bonsai_core::{Extraction, Language, LanguageDescriptor, LanguageSpec};

#[derive(Debug)]
pub struct Vue;

/// Only the script blocks score. A `v-if` chain is branching, but the metric is defined over
/// script code, and counting a template would make a component incomparable with the same
/// logic written in TypeScript.
pub static VUE: Vue = Vue;

impl LanguageDescriptor for Vue {
    fn spec(&self) -> &'static LanguageSpec {
        &VUE_SPEC
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["vue"]
    }

    fn compiled(&self) -> &'static Language {
        compiled_tsx()
    }

    fn extract(&self, source: &str) -> Option<Extraction> {
        Some(sfc::extract(source))
    }
}

pub static VUE_SPEC: LanguageSpec = bonsai_lang_ts::spec("vue");

/// The kind-id table is resolved per grammar, so these cannot be shared with `bonsai-lang-ts`'s
/// even though the spec behind them is the same.
pub(crate) fn compiled_typescript() -> &'static Language {
    bonsai_core::compiled_once!({
        VUE_SPEC
            .compile(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap_or_else(|errors| {
                panic!("Vue spec does not match the linked TypeScript grammar: {errors}")
            })
    })
}

pub(crate) fn compiled_tsx() -> &'static Language {
    bonsai_core::compiled_once!({
        VUE_SPEC
            .compile(tree_sitter_typescript::LANGUAGE_TSX.into())
            .unwrap_or_else(|errors| {
                panic!("Vue spec does not match the linked TSX grammar: {errors}")
            })
    })
}
