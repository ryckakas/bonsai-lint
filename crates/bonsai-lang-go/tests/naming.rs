//! A Go method is declared beside its type, not inside it, so its baseline key has to come from
//! the receiver. Package-level literals take their key from wherever they are bound.

mod common;

use bonsai_core::NameOrigin;
use common::findings;

fn names(source: &str) -> Vec<String> {
    findings(source)
        .into_iter()
        .map(|finding| finding.qualified_name())
        .collect()
}

fn origin_of(source: &str, qualified: &str) -> NameOrigin {
    findings(source)
        .into_iter()
        .find(|finding| finding.qualified_name() == qualified)
        .unwrap_or_else(|| panic!("no unit named {qualified} in:\n{source}"))
        .origin
}

#[test]
fn a_declared_function_uses_its_own_name() {
    let source = "package p\nfunc declared() {}\n";
    assert_eq!(names(source), ["declared"]);
    assert_eq!(origin_of(source, "declared"), NameOrigin::Declared);
}

#[test]
fn a_method_is_qualified_by_its_receiver_type() {
    let source =
        "package p\nfunc (s *Stack) Push(v int) {}\nfunc (s Stack) Len() int { return 0 }\n";
    assert_eq!(names(source), ["Stack::Push", "Stack::Len"]);
    assert_eq!(origin_of(source, "Stack::Push"), NameOrigin::Declared);
}

#[test]
fn a_generic_receiver_is_qualified_by_its_base_type() {
    let source = "package p\nfunc (s *Stack[T]) Push(v T) {}\nfunc (m Map[K, V]) Get(k K) {}\n";
    assert_eq!(names(source), ["Stack::Push", "Map::Get"]);
}

#[test]
fn an_unnamed_or_blank_receiver_still_qualifies_the_method() {
    let source = "package p\nfunc (Stack) Kind() {}\nfunc (_ *Queue) Kind() {}\n";
    assert_eq!(names(source), ["Stack::Kind", "Queue::Kind"]);
}

#[test]
fn same_named_methods_on_different_receivers_are_not_suffixed() {
    let source = "package p\nfunc (a A) String() string { return \"\" }\nfunc (b B) String() string { return \"\" }\n";
    assert_eq!(names(source), ["A::String", "B::String"]);
}

#[test]
fn repeated_init_functions_are_told_apart() {
    let source = "package p\nfunc init() {}\nfunc init() {}\nfunc _() {}\nfunc _() {}\n";
    assert_eq!(names(source), ["init", "init~2", "_", "_~2"]);
}

#[test]
fn a_method_named_init_is_an_ordinary_declaration() {
    let source = "package p\nfunc (s *S) init() {}\n";
    assert_eq!(origin_of(source, "S::init"), NameOrigin::Declared);
}

#[test]
fn a_package_level_literal_takes_the_name_it_is_bound_to() {
    let source = "package p\nvar handler = func() {}\n";
    assert_eq!(names(source), ["handler"]);
    assert_eq!(origin_of(source, "handler"), NameOrigin::Bound);
}

#[test]
fn a_multi_name_var_pairs_names_and_literals_by_position() {
    let source = "package p\nvar (\n\tfirst, second = func() {}, func() {}\n)\n";
    assert_eq!(names(source), ["first", "second"]);
}

#[test]
fn a_map_entry_is_named_by_its_key() {
    let source = "package p\nvar routes = map[string]func(){\n\t\"list\": func() {},\n\t`show`: func() {},\n}\n";
    assert_eq!(names(source), ["list", "show"]);
}

#[test]
fn a_lone_literal_argument_takes_the_binding_of_its_call() {
    let source = "package p\nvar handler = wrap(func() {})\n";
    assert_eq!(names(source), ["handler"]);
}

/// `_` discards the value, so two `var _ = …` lines would otherwise share one baseline key.
#[test]
fn a_blank_binding_falls_through_to_a_positional_key() {
    let source = "package p\nvar _ = register(func() {})\nvar _ = register(func() {})\n";
    assert_eq!(names(source), ["register#0", "register#0~2"]);
    assert_eq!(origin_of(source, "register#0"), NameOrigin::Positional);
}

#[test]
fn a_bodyless_declaration_is_not_a_unit() {
    let source = "package p\nfunc assembly(x int) int\n";
    assert!(names(source).is_empty());
}

#[test]
fn literals_inside_a_function_roll_up_into_it() {
    let source =
        "package p\nfunc outer() {\n\tf := func() {\n\t\tg := func() {}\n\t\tg()\n\t}\n\tf()\n}\n";
    assert_eq!(names(source), ["outer"]);
}
