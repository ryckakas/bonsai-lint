//! The kind-id table is resolved per grammar, so the Vue spec has to be proven against both
//! TypeScript dialects even though its kinds are the TypeScript ones.

use bonsai_testkit::GrammarFixture;

#[test]
fn vue_typescript_grammar_contract() {
    GrammarFixture {
        spec: &bonsai_lang_vue::VUE_SPEC,
        language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        source: include_str!("fixtures/grammar.ts"),
        either_of: &[],
    }
    .assert_contract();
}

#[test]
fn vue_tsx_grammar_contract() {
    GrammarFixture {
        spec: &bonsai_lang_vue::VUE_SPEC,
        language: tree_sitter_typescript::LANGUAGE_TSX.into(),
        source: include_str!("fixtures/grammar.ts"),
        either_of: &[],
    }
    .assert_contract();
}
