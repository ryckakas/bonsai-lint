//! Guards the declared node kinds and field names against a grammar upgrade renaming something
//! out from under the kind-id table, where it would become a silent zero rather than an error.

use bonsai_core::{FieldNames, KindSets, LanguageSpec, Role, SpecErrors};
use bonsai_lang_php::SPEC;
use bonsai_testkit::GrammarFixture;

#[test]
fn php_grammar_contract() {
    GrammarFixture {
        spec: &SPEC,
        language: tree_sitter_php::LANGUAGE_PHP.into(),
        source: include_str!("fixtures/grammar.php"),
        either_of: &[&[
            "anonymous_function",
            "anonymous_function_creation_expression",
        ]],
    }
    .assert_contract();
}

// The contract proves the real spec compiles; these prove that one the grammar no longer matches
// does not, and that the error names what to fix.
fn compile_errors(kinds: KindSets, fields: FieldNames) -> SpecErrors {
    let spec: &'static LanguageSpec = Box::leak(Box::new(LanguageSpec {
        kinds,
        fields,
        ..SPEC
    }));
    spec.compile(tree_sitter_php::LANGUAGE_PHP.into())
        .expect_err("a spec the grammar does not match should not compile")
}

#[test]
fn a_kind_missing_from_the_grammar_fails_compilation_and_is_named() {
    let errors = compile_errors(
        KindSets {
            unit: &["no_such_kind"],
            ..SPEC.kinds
        },
        FieldNames { ..SPEC.fields },
    );

    assert_eq!(
        errors,
        SpecErrors {
            unknown_kinds: vec!["no_such_kind"],
            ..SpecErrors::default()
        }
    );
    assert_eq!(
        errors.to_string(),
        "node kinds absent from the grammar: [\"no_such_kind\"]. "
    );
}

#[test]
fn a_field_missing_from_the_grammar_fails_compilation_and_is_named() {
    let errors = compile_errors(
        KindSets { ..SPEC.kinds },
        FieldNames {
            name: "no_such_field",
            control_header: &["no_such_header"],
            ..SPEC.fields
        },
    );

    assert_eq!(
        errors,
        SpecErrors {
            unknown_fields: vec!["no_such_field", "no_such_header"],
            ..SpecErrors::default()
        }
    );
    assert_eq!(
        errors.to_string(),
        "field names absent from the grammar: [\"no_such_field\", \"no_such_header\"]. "
    );
}

#[test]
fn a_kind_given_two_roles_fails_compilation_and_both_are_named() {
    let errors = compile_errors(
        KindSets {
            jump: &["if_statement"],
            ..SPEC.kinds
        },
        FieldNames { ..SPEC.fields },
    );

    assert_eq!(
        errors,
        SpecErrors {
            conflicting_roles: vec![("if_statement", Role::If, Role::Jump)],
            ..SpecErrors::default()
        }
    );
    assert_eq!(
        errors.to_string(),
        "kind if_statement claimed by both If and Jump. "
    );
}
