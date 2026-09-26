use std::borrow::Cow;

use bonsai_core::language::field;
use bonsai_core::naming::compact;
use bonsai_core::walk::if_parts_by_fields;
use bonsai_core::{
    Callee, FieldNames, Hooks, IfPart, KindSets, Language, LanguageDescriptor, LanguageSpec, Role,
    UnitName, UnitScope,
};
use tree_sitter::Node;

#[derive(Debug)]
pub struct Python;

pub static PYTHON: Python = Python;

impl LanguageDescriptor for Python {
    fn spec(&self) -> &'static LanguageSpec {
        &SPEC
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["py", "pyw"]
    }

    fn compiled(&self) -> &'static Language {
        compiled()
    }

    fn is_generated(&self, source: &str) -> bool {
        is_generated(source)
    }
}

pub static SPEC: LanguageSpec = LanguageSpec {
    id: "python",
    kinds: KindSets {
        unit: &["function_definition", "lambda"],
        container: &["class_definition"],
        // A comprehension has a scope of its own and is what JavaScript writes as a callback
        // chain, so it nests like a closure without ever being a unit.
        nesting_function: &[
            "function_definition",
            "lambda",
            "list_comprehension",
            "set_comprehension",
            "dictionary_comprehension",
            "generator_expression",
        ],
        if_statement: &["if_statement"],
        else_if_clause: &["elif_clause"],
        else_clause: &["else_clause"],
        nesting_control: &[
            "conditional_expression",
            "match_statement",
            "for_statement",
            "while_statement",
            "except_clause",
        ],
        // Python has no labels, so break and continue are always free.
        jump: &[],
        unconditional_jump: &[],
        logical: &["boolean_operator"],
        parenthesis: &["parenthesized_expression"],
        call: &["call"],
        comment: &["comment"],
        leading_trivia: &["decorator"],
        preamble: &[],
    },
    fields: FieldNames {
        name: "name",
        body: "body",
        condition: "condition",
        if_then: "consequence",
        if_alternative: "alternative",
        else_body: Some("body"),
        logical_left: "left",
        logical_right: "right",
        logical_operator: "operator",
        control_header: &[],
    },
    hooks: &PythonHooks,
    optional_kinds: &[],
};

#[derive(Debug)]
struct PythonHooks;

impl Hooks for PythonHooks {
    fn normalize_logical_operator(&self, operator: &str) -> Option<&'static str> {
        normalize_logical_operator(operator)
    }

    fn is_penalized_jump(&self, _node: Node<'_>, _src: &[u8]) -> bool {
        false
    }

    fn resolve_callee<'t>(&self, node: Node<'t>) -> Option<Callee<'t>> {
        resolve_callee(node)
    }

    fn unit_name(&self, node: Node<'_>, src: &[u8]) -> UnitName {
        unit_name(node, src)
    }

    fn unit_scope(&self, _node: Node<'_>, _src: &[u8], container: Option<&str>) -> UnitScope {
        unit_scope(container)
    }

    fn container_name(&self, node: Node<'_>, src: &[u8]) -> Option<String> {
        text(node.child_by_field_name("name")?, src)
    }

    fn suppression_anchors<'t>(&self, node: Node<'t>, src: &[u8], visit: &mut dyn FnMut(Node<'t>)) {
        suppression_anchors(node, src, visit);
    }

    fn if_parts<'t>(&self, node: Node<'t>, lang: &Language, visit: &mut dyn FnMut(IfPart<'t>)) {
        if_parts(node, lang, visit);
    }

    fn unit_body<'t>(&self, node: Node<'t>, lang: &Language) -> Option<Node<'t>> {
        field(node, lang.fields.body).filter(|body| !is_declaration_only(*body))
    }

    fn is_control_header(&self, control: Node<'_>, child: Node<'_>) -> bool {
        is_control_header(control, child)
    }
}

fn compiled() -> &'static Language {
    bonsai_core::compiled_once!({
        SPEC.compile(tree_sitter_python::LANGUAGE.into())
            .unwrap_or_else(|errors| {
                panic!("Python spec does not match the linked grammar: {errors}")
            })
    })
}

fn normalize_logical_operator(operator: &str) -> Option<&'static str> {
    match operator {
        "and" => Some("&&"),
        "or" => Some("||"),
        _ => None,
    }
}

/// `super().m()` has a call as its receiver, which never matches a unit's own spellings.
fn resolve_callee(node: Node<'_>) -> Option<Callee<'_>> {
    let function = node.child_by_field_name("function")?;
    match function.kind() {
        "identifier" => Some(Callee {
            receiver: None,
            name: function,
        }),
        "attribute" => Some(Callee {
            receiver: function.child_by_field_name("object"),
            name: function.child_by_field_name("attribute")?,
        }),
        _ => None,
    }
}

/// Inside a class a bare name resolves to a global, never to the method, which is reached
/// through `self`, `cls` or the class itself.
fn unit_scope(container: Option<&str>) -> UnitScope {
    let Some(class) = container.and_then(|path| path.rsplit("::").next()) else {
        return UnitScope {
            container: None,
            self_receivers: Vec::new(),
            bare_call_recurses: true,
        };
    };
    UnitScope {
        container: None,
        self_receivers: vec![
            Cow::Borrowed("self"),
            Cow::Borrowed("cls"),
            Cow::Owned(class.to_string()),
        ],
        bare_call_recurses: false,
    }
}

/// `elif` holds its branch in `consequence`, where the default reads an else-if's `body`.
fn if_parts<'t>(node: Node<'t>, lang: &Language, visit: &mut dyn FnMut(IfPart<'t>)) {
    if lang.role(node) != Role::ElseIf {
        if_parts_by_fields(node, lang, visit);
        return;
    }
    if let Some(condition) = field(node, lang.fields.condition) {
        visit(IfPart::Header(condition));
    }
    if let Some(then) = field(node, lang.fields.if_then) {
        visit(IfPart::Then(then));
    }
}

/// A body of nothing but `...`, after an optional docstring, is Python's spelling of a bodyless
/// declaration: a `typing.overload` stub, a protocol or an abstract signature.
fn is_declaration_only(body: Node<'_>) -> bool {
    let mut cursor = body.walk();
    let mut statements = body
        .named_children(&mut cursor)
        .filter(|statement| statement.kind() != "comment")
        .peekable();
    statements.next_if(|first| is_expression(*first, "string"));
    statements.peek().is_some() && statements.all(|statement| is_expression(statement, "ellipsis"))
}

/// An expression used as a statement, with or without the `expression_statement` wrapper the
/// grammar is dropping.
fn is_expression(statement: Node<'_>, kind: &str) -> bool {
    if statement.kind() == kind {
        return true;
    }
    statement.kind() == "expression_statement"
        && statement.named_child_count() == 1
        && statement
            .named_child(0)
            .is_some_and(|inner| inner.kind() == kind)
}

/// `a if c else b` leaves its condition unfielded between the two branches.
fn is_control_header(control: Node<'_>, child: Node<'_>) -> bool {
    if control.kind() != "conditional_expression" {
        return false;
    }
    let mut previous = child.prev_sibling();
    while let Some(extra) = previous.filter(Node::is_extra) {
        previous = extra.prev_sibling();
    }
    previous.is_some_and(|token| token.kind() == "if")
}

fn unit_name(node: Node<'_>, src: &[u8]) -> UnitName {
    if node.kind() == "lambda" {
        return lambda_name(node, src);
    }
    let Some(name) = node
        .child_by_field_name("name")
        .and_then(|name| text(name, src))
    else {
        return UnitName::anonymous();
    };
    match accessor(node, &name, src) {
        Some(kind) => UnitName::declared(name).with_signature(format!(".{kind}")),
        None => UnitName::declared(name),
    }
}

/// `@total.setter` redefines `total`, so the accessor it declares tells the two apart.
fn accessor(node: Node<'_>, name: &str, src: &[u8]) -> Option<String> {
    let wrapper = node
        .parent()
        .filter(|parent| parent.kind() == "decorated_definition")?;
    let mut cursor = wrapper.walk();
    let found = wrapper
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "decorator")
        .filter_map(|decorator| decorator.named_child(0))
        .filter(|target| target.kind() == "attribute")
        .find_map(|target| {
            let object = text(target.child_by_field_name("object")?, src)?;
            (object == name).then(|| text(target.child_by_field_name("attribute")?, src))?
        });
    found
}

fn lambda_name(node: Node<'_>, src: &[u8]) -> UnitName {
    if let Some(name) = bound_name(node, src) {
        return UnitName::bound(name);
    }
    if let Some(name) = positional_name(node, src) {
        return UnitName::positional(name);
    }
    UnitName::anonymous()
}

/// Inside a function a lambda rolls up, so only module and class level bindings reach here:
/// an assignment, or a call handing its own binding to a lone lambda argument, as
/// `items = field(default_factory=lambda: [])` does.
fn bound_name(node: Node<'_>, src: &[u8]) -> Option<String> {
    let mut current = node;
    loop {
        let parent = current.parent()?;
        match parent.kind() {
            "assignment" if is_field(parent, "right", current) => {
                return text(parent.child_by_field_name("left")?, src).map(|left| compact(&left));
            }
            "keyword_argument" if is_field(parent, "value", current) => current = parent,
            "argument_list" => current = binding_call(parent, current)?,
            "parenthesized_expression" => current = parent,
            _ => return None,
        }
    }
}

fn is_field(parent: Node<'_>, name: &str, child: Node<'_>) -> bool {
    parent
        .child_by_field_name(name)
        .is_some_and(|value| value.id() == child.id())
}

fn binding_call<'t>(arguments: Node<'t>, current: Node<'t>) -> Option<Node<'t>> {
    let mut cursor = arguments.walk();
    let mut lambdas = arguments
        .named_children(&mut cursor)
        .filter(|argument| is_lambda_argument(*argument));
    let sole = lambdas
        .next()
        .is_some_and(|first| first.id() == current.id())
        && lambdas.next().is_none();
    sole.then(|| arguments.parent())
        .flatten()
        .filter(|call| call.kind() == "call")
}

fn is_lambda_argument(argument: Node<'_>) -> bool {
    match argument.kind() {
        "lambda" => true,
        "keyword_argument" => argument
            .child_by_field_name("value")
            .is_some_and(|value| value.kind() == "lambda"),
        _ => false,
    }
}

/// A callback that is nobody's value takes the call it belongs to plus its argument position:
/// `atexit.register#0`.
fn positional_name(node: Node<'_>, src: &[u8]) -> Option<String> {
    let argument = node
        .parent()
        .filter(|parent| parent.kind() == "keyword_argument")
        .unwrap_or(node);
    let arguments = argument
        .parent()
        .filter(|parent| parent.kind() == "argument_list")?;
    let call = arguments.parent().filter(|call| call.kind() == "call")?;
    let callee =
        compact(std::str::from_utf8(&src[call.start_byte()..arguments.start_byte()]).ok()?);
    if callee.is_empty() || callee.contains(['(', '[', '{']) {
        return None;
    }

    let mut cursor = arguments.walk();
    let index = arguments
        .named_children(&mut cursor)
        .filter(|candidate| candidate.kind() != "comment")
        .position(|candidate| candidate.id() == argument.id())?;
    Some(format!("{callee}#{index}"))
}

/// The def itself, then the decorated definition wrapping it, whose decorators a marker sits
/// above. A bound lambda answers from the statement that binds it.
fn suppression_anchors<'t>(node: Node<'t>, src: &[u8], visit: &mut dyn FnMut(Node<'t>)) {
    visit(node);
    let mut outermost = node;
    if let Some(wrapper) = node
        .parent()
        .filter(|parent| parent.kind() == "decorated_definition")
    {
        visit(wrapper);
        outermost = wrapper;
    }
    if node.kind() == "lambda" && bound_name(node, src).is_some() {
        outermost = statement(node);
        visit(outermost);
    }
    if let Some(block) = opened_block(outermost) {
        visit(block);
    }
}

/// A comment above the first statement of a block lands before the block, in the class or `if`
/// holding it, rather than beside the statement it describes.
fn opened_block(node: Node<'_>) -> Option<Node<'_>> {
    let block = node.parent().filter(|parent| parent.kind() == "block")?;
    let mut cursor = block.walk();
    let first = block
        .named_children(&mut cursor)
        .find(|child| child.kind() != "comment")?;
    (first.id() == node.id()).then_some(block)
}

/// Climbs to the child of the enclosing block, without naming `expression_statement`, which
/// the grammar is turning into a hidden supertype.
fn statement(node: Node<'_>) -> Node<'_> {
    let mut current = node;
    while let Some(parent) = current
        .parent()
        .filter(|parent| !matches!(parent.kind(), "module" | "block"))
    {
        current = parent;
    }
    current
}

/// Python has no single convention, but protobuf, gRPC and Thrift output all say both phrases
/// in the comments before the first line of code. A Django migration says only "Generated",
/// since it is meant to be edited.
#[must_use]
pub fn is_generated(source: &str) -> bool {
    let header = header_comments(source).to_lowercase();
    let header = header.split_whitespace().collect::<Vec<_>>().join(" ");
    header.contains("generated") && header.contains("do not edit")
}

fn header_comments(source: &str) -> String {
    let mut header = String::new();
    for line in source.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(comment) = line.strip_prefix('#') else {
            break;
        };
        header.push_str(comment);
        header.push('\n');
    }
    header
}

fn text(node: Node<'_>, src: &[u8]) -> Option<String> {
    node.utf8_text(src).ok().map(ToString::to_string)
}
