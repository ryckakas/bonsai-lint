//! Guards the declared node kinds and field names against a grammar upgrade renaming something
//! out from under the kind-id table, where it would become a silent zero rather than an error.

use bonsai_testkit::GrammarFixture;

#[test]
fn php_grammar_contract() {
    GrammarFixture {
        spec: &bonsai_lang_php::SPEC,
        language: tree_sitter_php::LANGUAGE_PHP.into(),
        source: include_str!("fixtures/grammar.php"),
        either_of: &[&[
            "anonymous_function",
            "anonymous_function_creation_expression",
        ]],
    }
    .assert_contract();
}
