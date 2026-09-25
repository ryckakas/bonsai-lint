//! Guards the declared node kinds and field names against a grammar upgrade renaming something
//! out from under the kind-id table, where it would become a silent zero rather than an error.

use bonsai_testkit::GrammarFixture;

#[test]
fn java_grammar_contract() {
    GrammarFixture {
        spec: &bonsai_lang_java::SPEC,
        language: tree_sitter_java::LANGUAGE.into(),
        source: include_str!("fixtures/Grammar.java"),
        either_of: &[],
    }
    .assert_contract();
}
