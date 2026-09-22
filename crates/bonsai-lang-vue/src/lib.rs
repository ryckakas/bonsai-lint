mod sfc;

#[cfg(not(target_family = "wasm"))]
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

// tree-sitter gates `unsafe impl Sync for Language` behind `cfg(not(target_family = "wasm"))`,
// so a `static OnceLock<Language>` cannot exist on wasm32. That target is single-threaded, so a
// thread-local cell plus a one-time leak yields the same `&'static` with no unsafe and no
// behavioural difference. Native builds keep the `OnceLock` exactly as before.
#[cfg(target_family = "wasm")]
macro_rules! compiled_once {
    ($build:expr) => {{
        thread_local! {
            static COMPILED: std::cell::OnceCell<&'static Language> =
                const { std::cell::OnceCell::new() };
        }
        COMPILED.with(|cell| *cell.get_or_init(|| Box::leak(Box::new($build))))
    }};
}

#[cfg(not(target_family = "wasm"))]
macro_rules! compiled_once {
    ($build:expr) => {{
        static COMPILED: OnceLock<Language> = OnceLock::new();
        COMPILED.get_or_init(|| $build)
    }};
}

/// The kind-id table is resolved per grammar, so these cannot be shared with `bonsai-lang-ts`'s
/// even though the spec behind them is the same.
pub(crate) fn compiled_typescript() -> &'static Language {
    compiled_once!({
        VUE_SPEC
            .compile(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap_or_else(|errors| {
                panic!("Vue spec does not match the linked TypeScript grammar: {errors}")
            })
    })
}

pub(crate) fn compiled_tsx() -> &'static Language {
    compiled_once!({
        VUE_SPEC
            .compile(tree_sitter_typescript::LANGUAGE_TSX.into())
            .unwrap_or_else(|errors| {
                panic!("Vue spec does not match the linked TSX grammar: {errors}")
            })
    })
}
