//! Guards the declared node kinds and field names against a grammar upgrade renaming something
//! out from under the kind-id table, where it would become a silent zero rather than an error.

use bonsai_testkit::GrammarFixture;

#[test]
fn typescript_grammar_contract() {
    GrammarFixture {
        spec: &bonsai_lang_ts::SPEC,
        language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        source: include_str!("fixtures/grammar.ts"),
        either_of: &[],
    }
    .assert_contract();
}

#[test]
fn tsx_grammar_contract() {
    GrammarFixture {
        spec: &bonsai_lang_ts::SPEC,
        language: tree_sitter_typescript::LANGUAGE_TSX.into(),
        source: include_str!("fixtures/grammar.ts"),
        either_of: &[],
    }
    .assert_contract();
}
