//! Most JavaScript functions are anonymous at the point of declaration, so a unit's baseline
//! key has to come from wherever it is bound.

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
    assert!(names("function declared() {}").contains(&"declared".to_string()));
    assert_eq!(
        origin_of("function declared() {}", "declared"),
        NameOrigin::Declared
    );
}

#[test]
fn an_arrow_takes_the_name_it_is_bound_to() {
    let source = "const bound = () => {};";
    assert!(names(source).contains(&"bound".to_string()));
    assert_eq!(origin_of(source, "bound"), NameOrigin::Bound);
}

#[test]
fn a_method_is_qualified_by_its_class() {
    assert!(names("class C { method() {} }").contains(&"C::method".to_string()));
}

#[test]
fn a_class_field_arrow_is_qualified_by_its_class() {
    assert!(names("class F { field = () => {}; }").contains(&"F::field".to_string()));
}

#[test]
fn an_object_literal_is_named_by_its_binding() {
    let source = "const api = { onClick: function () {} };";
    assert!(
        names(source).contains(&"api::onClick".to_string()),
        "{:?}",
        names(source)
    );
}

#[test]
fn a_default_export_is_named_default() {
    assert!(names("export default function () {}").contains(&"default".to_string()));
}

/// A callback that is nobody's value still needs a stable key.
#[test]
fn a_bare_callback_takes_its_call_and_position() {
    let source = "app.get('/x', (req, res) => {});";
    assert!(
        names(source).contains(&"app.get#1".to_string()),
        "{:?}",
        names(source)
    );
    assert_eq!(origin_of(source, "app.get#1"), NameOrigin::Positional);
}

/// Two callbacks on the same call would otherwise share a baseline key.
#[test]
fn colliding_positional_names_get_a_suffix() {
    let source = "app.get('/a', () => {});\napp.get('/b', () => {});";
    let names = names(source);
    assert!(names.contains(&"app.get#1".to_string()), "{names:?}");
    assert!(names.contains(&"app.get#1~2".to_string()), "{names:?}");
}

/// Declared names that collide are a genuine duplicate in the source, not a namer artefact, so
/// they are left alone.
#[test]
fn declared_names_are_not_suffixed() {
    let source = "class A { m() {} }\nclass B { m() {} }";
    let names = names(source);
    assert!(names.contains(&"A::m".to_string()), "{names:?}");
    assert!(names.contains(&"B::m".to_string()), "{names:?}");
}

/// Rollup means a nested callback is not a unit at all, so it never needs a key.
#[test]
fn nested_callbacks_do_not_become_units() {
    let source = "function outer() { items.forEach(item => { if (item) { f(); } }); }";
    assert_eq!(names(source), vec!["outer".to_string()]);
}

/// An immediately-invoked function's callee is the function itself, so stringifying it would
/// turn a whole minified library into one baseline key.
#[test]
fn an_iife_does_not_become_a_giant_positional_name() {
    let source = "(function (factory) { if (a) { f(); } })(jQuery);";
    let names = names(source);
    assert_eq!(
        names,
        vec![bonsai_core::ANONYMOUS_UNIT.to_string()],
        "{names:?}"
    );
}

#[test]
fn positional_names_stay_short() {
    for name in names("describe('a very long test description here', () => { f(); });") {
        assert!(name.len() < 40, "unreasonably long unit key: {name}");
    }
}

/// A store or component factory should take the name the call is bound to, not a positional key
/// invented from the callee.
#[test]
fn a_factory_call_passes_its_binding_to_the_callback() {
    let source = "export const useCartStore = defineStore('cart', () => { if (a) { f(); } });";
    assert!(
        names(source).contains(&"useCartStore".to_string()),
        "{:?}",
        names(source)
    );
}

#[test]
fn nested_wrapper_calls_still_reach_the_binding() {
    let source = "const Button = React.memo(forwardRef((props, ref) => { if (a) { f(); } }));";
    assert!(
        names(source).contains(&"Button".to_string()),
        "{:?}",
        names(source)
    );
}

/// A call that is nobody's value has no name to borrow, so the positional key stands.
#[test]
fn a_bare_call_does_not_borrow_a_name() {
    let source = "app.get('/x', (req, res) => { if (a) { f(); } });";
    assert!(
        names(source).contains(&"app.get#1".to_string()),
        "{:?}",
        names(source)
    );
}

/// Two callbacks in one call are ambiguous, so neither may claim the binding.
#[test]
fn an_ambiguous_call_keeps_positional_keys() {
    let source = "const pair = combine(() => { if (a) { f(); } }, () => { if (b) { g(); } });";
    let names = names(source);
    assert!(names.iter().any(|n| n.starts_with("combine#")), "{names:?}");
}
