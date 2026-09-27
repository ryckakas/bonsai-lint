//! The grammar contract checks each kind `KindSets::all` yields against the fixture and the
//! grammar, so a list it skipped would go unchecked in every language at once.

use bonsai_core::KindSets;

#[test]
fn all_yields_the_kinds_of_every_list() {
    let kinds = KindSets {
        unit: &["unit"],
        container: &["container"],
        nesting_function: &["nesting_function"],
        if_statement: &["if_statement"],
        else_if_clause: &["else_if_clause"],
        else_clause: &["else_clause"],
        nesting_control: &["nesting_control"],
        jump: &["jump"],
        unconditional_jump: &["unconditional_jump"],
        logical: &["logical"],
        parenthesis: &["parenthesis"],
        call: &["call"],
        comment: &["comment"],
        leading_trivia: &["leading_trivia"],
        preamble: &["preamble"],
    };

    let mut all: Vec<&str> = kinds.all().collect();
    all.sort_unstable();

    assert_eq!(
        all,
        [
            "call",
            "comment",
            "container",
            "else_clause",
            "else_if_clause",
            "if_statement",
            "jump",
            "leading_trivia",
            "logical",
            "nesting_control",
            "nesting_function",
            "parenthesis",
            "preamble",
            "unconditional_jump",
            "unit",
        ]
    );
}
