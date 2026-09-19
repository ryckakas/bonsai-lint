//! A closure at file scope is a unit like a `function`, so it needs a key from wherever it is
//! bound, and the same shapes as the TypeScript namer must produce the same kinds of key.

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

fn line_of(source: &str, qualified: &str) -> usize {
    findings(source)
        .into_iter()
        .find(|finding| finding.qualified_name() == qualified)
        .unwrap_or_else(|| panic!("no unit named {qualified} in:\n{source}"))
        .line
}

#[test]
fn a_closure_takes_the_variable_it_is_assigned_to() {
    let source = "<?php\n$handler = function () { if ($a) { echo 1; } };\n";
    assert!(
        names(source).contains(&"handler".to_string()),
        "{:?}",
        names(source)
    );
    assert_eq!(origin_of(source, "handler"), NameOrigin::Bound);
}

#[test]
fn an_arrow_function_takes_its_binding_too() {
    let source = "<?php\n$double = fn ($x) => $x * 2;\n";
    assert!(
        names(source).contains(&"double".to_string()),
        "{:?}",
        names(source)
    );
}

#[test]
fn a_property_assignment_keeps_its_receiver() {
    let source = "<?php\n$this->handler = function () {};\n";
    assert!(
        names(source).contains(&"$this->handler".to_string()),
        "{:?}",
        names(source)
    );
}

#[test]
fn an_array_element_is_named_by_its_key() {
    let source = "<?php\n$api = ['onClick' => function () {}];\n";
    assert!(
        names(source).contains(&"onClick".to_string()),
        "{:?}",
        names(source)
    );
}

#[test]
fn a_bare_callback_takes_its_call_and_position() {
    let route = "<?php\nRoute::get('/x', function () {});\n";
    assert!(
        names(route).contains(&"Route::get#1".to_string()),
        "{:?}",
        names(route)
    );
    assert_eq!(origin_of(route, "Route::get#1"), NameOrigin::Positional);

    let map = "<?php\narray_map(fn ($x) => $x, $xs);\n";
    assert!(
        names(map).contains(&"array_map#0".to_string()),
        "{:?}",
        names(map)
    );

    let member = "<?php\n$router->get('/x', function () {});\n";
    assert!(
        names(member).contains(&"$router->get#1".to_string()),
        "{:?}",
        names(member)
    );
}

#[test]
fn a_factory_call_passes_its_binding_to_the_callback() {
    let source = "<?php\n$handler = Closure::fromCallable(function () {});\n";
    assert!(
        names(source).contains(&"handler".to_string()),
        "{:?}",
        names(source)
    );
}

#[test]
fn colliding_positional_names_get_a_suffix() {
    let source = "<?php\nRoute::get('/a', function () {});\nRoute::get('/b', function () {});\n";
    let names = names(source);
    assert!(names.contains(&"Route::get#1".to_string()), "{names:?}");
    assert!(names.contains(&"Route::get#1~2".to_string()), "{names:?}");
}

/// The callee of an immediately-invoked closure is the closure itself, so stringifying it would
/// turn the whole body into a key.
#[test]
fn an_iife_stays_anonymous() {
    let source = "<?php\n(function () { if ($a) { echo 1; } })();\n";
    assert_eq!(names(source), vec![bonsai_core::ANONYMOUS_UNIT.to_string()]);
}

#[test]
fn nested_closures_roll_up_into_the_enclosing_unit() {
    let source =
        "<?php\nfunction outer() { array_map(function ($x) { if ($x) { echo 1; } }, $xs); }\n";
    assert_eq!(names(source), vec!["outer".to_string()]);
}

/// Correct by omission: a method without a body has nothing to score, so it is not a unit.
#[test]
fn bodyless_methods_are_not_units() {
    let source = "<?php\ninterface I { public function m(); }\nabstract class A { abstract public function n(); public function o() {} }\n";
    let names = names(source);
    assert!(names.contains(&"A::o".to_string()), "{names:?}");
    assert!(!names.contains(&"I::m".to_string()), "{names:?}");
    assert!(!names.contains(&"A::n".to_string()), "{names:?}");
}

#[test]
fn an_attribute_does_not_move_the_reported_line() {
    let function = "<?php\n#[Route('/x')]\nfunction handler() {}\n";
    assert_eq!(line_of(function, "handler"), 3);

    let method =
        "<?php\nclass A {\n    #[Route('/x')]\n    #[Deprecated]\n    public function m() {}\n}\n";
    assert_eq!(line_of(method, "A::m"), 5);
}
