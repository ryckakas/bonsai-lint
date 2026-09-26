//! A Python unit is keyed by its name and the classes around it. Property accessors redefine one
//! name on purpose, so the accessor they declare joins the key; lambdas take their key from
//! wherever they are bound.

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
fn a_function_is_keyed_by_its_name() {
    let source = "def process(order):\n    pass\n\nasync def fetch(url):\n    pass\n";
    assert_eq!(names(source), ["process", "fetch"]);
    assert_eq!(origin_of(source, "process"), NameOrigin::Declared);
}

#[test]
fn a_method_is_qualified_by_its_classes() {
    let source = "class OrderService:\n    def process(self, o):\n        pass\n\nclass Outer:\n    class Inner:\n        def m(self):\n            pass\n";
    assert_eq!(names(source), ["OrderService::process", "Outer::Inner::m"]);
}

#[test]
fn one_method_name_in_two_classes_is_not_suffixed() {
    let source = "class A:\n    def run(self):\n        pass\n\nclass B:\n    def run(self):\n        pass\n";
    assert_eq!(names(source), ["A::run", "B::run"]);
}

#[test]
fn a_decorated_method_keys_like_an_undecorated_one() {
    let source = "class C:\n    @staticmethod\n    def build():\n        pass\n\n    @app.route(\"/x\")\n    def view(self):\n        pass\n";
    assert_eq!(names(source), ["C::build", "C::view"]);
}

#[test]
fn property_accessors_are_told_apart_by_the_accessor_they_declare() {
    let source = "class Cart:\n    @property\n    def total(self):\n        return 1\n\n    @total.setter\n    def total(self, value):\n        pass\n\n    @total.deleter\n    def total(self):\n        pass\n\n    @total.getter\n    def total(self):\n        return 2\n";
    assert_eq!(
        names(source),
        [
            "Cart::total",
            "Cart::total.setter",
            "Cart::total.deleter",
            "Cart::total.getter"
        ]
    );
}

#[test]
fn a_decorator_on_another_name_does_not_join_the_key() {
    let source = "class Cart:\n    @other.setter\n    def total(self, value):\n        pass\n";
    assert_eq!(names(source), ["Cart::total"]);
}

/// A body of nothing but `...` declares a signature without implementing it, like a Java
/// interface method, so overload stubs don't collide with the implementation they describe.
#[test]
fn a_declaration_only_body_is_not_a_unit() {
    let source = "@overload\ndef parse(x: int) -> int: ...\n\n@typing.overload\ndef parse(x: str) -> str:\n    ...\n\ndef parse(x):\n    return x\n\nclass Reader(Protocol):\n    def read(self) -> bytes:\n        \"\"\"Reads it.\"\"\"\n        ...\n";
    assert_eq!(names(source), ["parse"]);

    let docstring_forms = "@overload\ndef parse(x: int) -> int:\n    (\"Parenthesized.\")\n    ...\n\n@overload\ndef parse(x: str) -> str:\n    \"Implicitly \" \"concatenated.\"\n    ...\n\ndef parse(x):\n    return x\n";
    assert_eq!(names(docstring_forms), ["parse"]);
}

#[test]
fn a_placeholder_or_docstring_body_is_still_a_unit() {
    let source = "def todo():\n    pass\n\ndef documented():\n    \"\"\"Returns None.\"\"\"\n";
    assert_eq!(names(source), ["todo", "documented"]);
}

#[test]
fn nested_functions_lambdas_comprehensions_and_local_classes_roll_up() {
    let source = "def outer():\n    def inner():\n        pass\n    g = lambda: 0\n    ys = [x for x in xs]\n    class Local:\n        def m(self):\n            pass\n";
    assert_eq!(names(source), ["outer"]);
}

#[test]
fn a_bound_lambda_takes_its_binding() {
    let module = "handler = lambda e: e\n";
    assert_eq!(names(module), ["handler"]);
    assert_eq!(origin_of(module, "handler"), NameOrigin::Bound);

    let attribute =
        "class C:\n    key = lambda self: 1\n    items: list = field(default_factory=lambda: [])\n";
    assert_eq!(names(attribute), ["C::key", "C::items"]);

    let wrapped = "S = memoize(lambda: 0)\n";
    assert_eq!(names(wrapped), ["S"]);
}

#[test]
fn a_callback_takes_its_call_and_argument_position() {
    let source = "atexit.register(lambda: 0)\nsorted(xs, key=lambda v: v)\n";
    assert_eq!(names(source), ["atexit.register#0", "sorted#1"]);
    assert_eq!(
        origin_of(source, "atexit.register#0"),
        NameOrigin::Positional
    );
}

#[test]
fn a_lambda_nobody_names_is_anonymous() {
    let source = "HANDLERS = {\"a\": lambda: 1}\n";
    assert_eq!(names(source), ["<anonymous>"]);
}

/// Accepted collisions: both definitions share a key, as the roadmap records for any language.
#[test]
fn a_conditional_redefinition_and_singledispatch_implementations_collide() {
    let conditional = "if WINDOWS:\n    def open_file(p):\n        pass\nelse:\n    def open_file(p):\n        pass\n";
    assert_eq!(names(conditional), ["open_file", "open_file", "<toplevel>"]);

    let dispatch =
        "@fun.register\ndef _(arg: int):\n    pass\n\n@fun.register\ndef _(arg: list):\n    pass\n";
    assert_eq!(names(dispatch), ["_", "_"]);
}
