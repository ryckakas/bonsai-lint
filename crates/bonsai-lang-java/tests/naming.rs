//! Java declares one name many times on purpose, so a method's key carries its parameter types
//! as well as its name. Class-level lambdas take their key from wherever they are bound.

mod common;

use bonsai_core::NameOrigin;
use common::{findings, findings_in_a_grammar_gap};

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
fn a_method_is_qualified_by_its_class_and_signature() {
    let source = "class OrderService { void process(Order o, User u) {} }\n";
    assert_eq!(names(source), ["OrderService::process(Order, User)"]);
    assert_eq!(
        origin_of(source, "OrderService::process(Order, User)"),
        NameOrigin::Declared
    );
}

#[test]
fn overloads_are_told_apart_by_their_parameter_types() {
    let source = "class S {\n  void process(Order o, User u) {}\n  void process(Order o) {}\n  void process() {}\n}\n";
    assert_eq!(
        names(source),
        [
            "S::process(Order, User)",
            "S::process(Order)",
            "S::process()"
        ]
    );
}

#[test]
fn generics_annotations_and_modifiers_are_dropped_from_the_signature() {
    let source =
        "class C { <T> void put(@NonNull final Map<K, List<V>> m, T[] xs, String... rest) {} }\n";
    assert_eq!(names(source), ["C::put(Map, T[], String...)"]);
}

#[test]
fn a_qualified_and_an_imported_type_key_alike() {
    let qualified = "class C { void f(java.util.List<String> xs) {} }\n";
    let imported = "class C { void f(List<String> xs) {} }\n";
    assert_eq!(names(qualified), ["C::f(List)"]);
    assert_eq!(names(imported), ["C::f(List)"]);
}

/// Keying by simple name keeps an import-style change from re-keying a baseline, at the price
/// of overloads that differ only by package.
#[test]
fn overloads_on_same_named_types_from_two_packages_collide() {
    let source = "class C {\n  void f(java.util.Date d) {}\n  void f(java.sql.Date d) {}\n}\n";
    assert_eq!(names(source), ["C::f(Date)", "C::f(Date)"]);
}

#[test]
fn array_dimensions_are_kept_however_they_are_written() {
    let source = "class C { void f(int[][] grid, String args[], List<String>[] lists, String @A [] tagged) {} }\n";
    assert_eq!(names(source), ["C::f(int[][], String[], List[], String[])"]);
}

#[test]
fn a_receiver_parameter_is_not_part_of_the_key() {
    let source = "class C { void f(C this, int x) {} }\n";
    assert_eq!(names(source), ["C::f(int)"]);
}

#[test]
fn a_constructor_is_keyed_by_its_class_name() {
    let source = "class Point {\n  Point(int x, int y) {}\n  Point() { this(0, 0); }\n}\n";
    assert_eq!(names(source), ["Point::Point(int, int)", "Point::Point()"]);
}

#[test]
fn a_compact_constructor_takes_the_record_components() {
    let source = "record Point(int x, int y) {\n  Point { if (x > y) throw new E(); }\n  Point(int x) { this(x, x); }\n}\n";
    assert_eq!(
        names(source),
        ["Point::Point(int, int)", "Point::Point(int)"]
    );
}

#[test]
fn nested_classes_extend_the_path() {
    let source = "class Outer { static class Inner { void m() {} } }\n";
    assert_eq!(names(source), ["Outer::Inner::m()"]);
}

#[test]
fn same_signature_in_two_classes_is_not_suffixed() {
    let source = "class A { void m() {} }\nclass B { void m() {} }\n";
    assert_eq!(names(source), ["A::m()", "B::m()"]);
}

#[test]
fn a_bodyless_declaration_is_not_a_unit() {
    let source = "abstract class A {\n  abstract void m();\n  native void n();\n}\ninterface I { void m(); }\n";
    assert!(names(source).is_empty());
}

#[test]
fn interface_default_and_static_methods_are_units() {
    let source =
        "interface I {\n  default void d() {}\n  static void s() {}\n  void abstractOne();\n}\n";
    assert_eq!(names(source), ["I::d()", "I::s()"]);
}

#[test]
fn an_enum_constant_body_extends_the_path() {
    let source = "enum Op {\n  PLUS { int apply(int a, int b) { return a + b; } },\n  MINUS { int apply(int a, int b) { return a - b; } };\n  abstract int apply(int a, int b);\n  String describe() { return name(); }\n}\n";
    assert_eq!(
        names(source),
        [
            "Op::PLUS::apply(int, int)",
            "Op::MINUS::apply(int, int)",
            "Op::describe()"
        ]
    );
}

#[test]
fn a_field_anonymous_class_is_named_by_its_field() {
    let source = "class Outer {\n  static final Comparator<S> CMP = new Comparator<>() {\n    public int compare(S a, S b) { return 0; }\n  };\n}\n";
    assert_eq!(names(source), ["Outer::CMP::compare(S, S)"]);
}

#[test]
fn a_field_lambda_takes_the_field_name() {
    let source = "class Outer { private final Runnable handler = () -> {}; }\n";
    assert_eq!(names(source), ["Outer::handler"]);
    assert_eq!(origin_of(source, "Outer::handler"), NameOrigin::Bound);
}

#[test]
fn a_lone_lambda_argument_takes_the_binding_of_its_call() {
    let source = "class Outer {\n  static final Supplier<X> S = memoize(() -> new X());\n  static final Thread T = new Thread(() -> {});\n}\n";
    assert_eq!(names(source), ["Outer::S", "Outer::T"]);
}

#[test]
fn several_callbacks_in_one_call_take_their_positions() {
    let source = "class Outer {\n  static final Map<String, Runnable> M = Map.of(\"a\", () -> {}, \"b\", () -> {});\n}\n";
    assert_eq!(names(source), ["Outer::Map.of#1", "Outer::Map.of#3"]);
    assert_eq!(origin_of(source, "Outer::Map.of#1"), NameOrigin::Positional);
}

#[test]
fn anonymous_classes_passed_to_one_constructor_take_its_positions() {
    let source = "class C {\n  static final Dispatcher D = new Dispatcher(\n    new Runnable() { public void run() {} },\n    new Runnable() { public void run() {} }\n  );\n}\n";
    assert_eq!(
        names(source),
        ["C::new Dispatcher#0::run()", "C::new Dispatcher#1::run()"]
    );
}

#[test]
fn several_callbacks_in_one_constructor_take_its_type_and_their_positions() {
    let source =
        "class C {\n  static final Pair P = new java.util.Pair<>(() -> {}, () -> {});\n}\n";
    assert_eq!(names(source), ["C::new Pair#0", "C::new Pair#1"]);
    assert_eq!(origin_of(source, "C::new Pair#0"), NameOrigin::Positional);
}

#[test]
fn an_initializer_block_callback_takes_its_call_and_position() {
    let source = "class Outer { { register(() -> {}); } }\n";
    assert_eq!(names(source), ["Outer::register#0"]);
}

#[test]
fn an_initializer_assignment_names_its_lambda() {
    let source = "class Outer { Runnable r; { r = () -> {}; } }\n";
    assert_eq!(names(source), ["Outer::r"]);
}

#[test]
fn static_initializers_are_told_apart() {
    let source = "class Config {\n  static { a(); }\n  static { b(); }\n}\n";
    assert_eq!(names(source), ["Config::<static>", "Config::<static>~2"]);
    assert_eq!(
        origin_of(source, "Config::<static>"),
        NameOrigin::Positional
    );
}

#[test]
fn a_top_level_method_of_an_implicit_class_has_no_container() {
    let source = "void main() {}\n";
    assert_eq!(names(source), ["main()"]);
}

#[test]
fn lambdas_and_nested_classes_inside_a_method_roll_up() {
    let source = "class C {\n  void outer() {\n    Runnable r = () -> {};\n    class Local { void m() {} }\n    new Thread(new Runnable() { public void run() {} });\n  }\n}\n";
    assert_eq!(names(source), ["C::outer()"]);
}

/// The grammar rejects `String @Nullable ... hosts`, the way nullness-annotated code marks every
/// nullable varargs. The key keeps the parameter, so it neither collides with a real no-argument
/// overload nor re-keys once the grammar learns the syntax.
#[test]
fn an_annotated_varargs_parameter_keeps_its_type_despite_the_grammar_gap() {
    let source = "class C {\n  void set(String @Nullable ... hosts) {}\n  void set() {}\n  void log(@Nullable String t, @Nullable Object @Nullable ... args) {}\n  void all(Class<?> @Nullable ... types) {}\n}\n";
    let names: Vec<String> = findings_in_a_grammar_gap(source)
        .into_iter()
        .map(|finding| finding.qualified_name())
        .collect();
    assert_eq!(
        names,
        [
            "C::set(String...)",
            "C::set()",
            "C::log(String, Object...)",
            "C::all(Class...)"
        ]
    );
}
