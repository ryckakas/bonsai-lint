//! Guards the declared node kinds and field names against a grammar upgrade renaming something
//! out from under the kind-id table, where it would become a silent zero rather than an error.

use bonsai_testkit::GrammarFixture;

#[test]
fn go_grammar_contract() {
    GrammarFixture {
        spec: &bonsai_lang_go::SPEC,
        language: tree_sitter_go::LANGUAGE.into(),
        source: include_str!("fixtures/grammar.go"),
        either_of: &[],
    }
    .assert_contract();
}
