use std::collections::HashSet;

use bonsai_core::{LanguageSpec, Role};
use tree_sitter::{Language as TsLanguage, Node, Parser};

/// The contract a language crate must keep with the grammar it is compiled against. The
/// kind-id table turns a renamed node into a silent zero rather than a compile error, so these
/// checks are what make that design safe to rely on.
pub struct GrammarFixture {
    pub spec: &'static LanguageSpec,
    pub language: TsLanguage,
    pub source: &'static str,
    /// Groups where at least one spelling must be live, for kinds a grammar renamed between
    /// releases and where both spellings are declared.
    pub either_of: &'static [&'static [&'static str]],
}

impl std::fmt::Debug for GrammarFixture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GrammarFixture")
            .field("spec", &self.spec.id)
            .finish_non_exhaustive()
    }
}

impl GrammarFixture {
    pub fn assert_contract(&self) {
        let tree = self.parse();
        self.assert_fixture_parses_cleanly(&tree);
        self.assert_spec_compiles();
        self.assert_declared_kinds_are_observed(&tree);
        self.assert_either_of_groups_are_live(&tree);
        self.assert_no_alias_drift(&tree);
        self.assert_if_alternatives_are_else_kinds(&tree);
    }

    fn parse(&self) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&self.language)
            .expect("grammar should load");
        parser
            .parse(self.source, None)
            .expect("fixture should parse")
    }

    fn assert_fixture_parses_cleanly(&self, tree: &tree_sitter::Tree) {
        assert!(
            !tree.root_node().has_error(),
            "[{}] the fixture no longer parses cleanly; fix the fixture before trusting the \
             kind checks",
            self.spec.id
        );
    }

    /// Compilation resolves every kind and field name, so it subsumes the old "does this string
    /// still exist" test and additionally catches a renamed field, which would otherwise zero
    /// out every construct that reads it.
    fn assert_spec_compiles(&self) {
        if let Err(errors) = self.spec.compile(self.language.clone()) {
            panic!(
                "[{}] spec does not match the grammar: {errors}",
                self.spec.id
            );
        }
    }

    /// A kind can resolve in the symbol table yet no longer be produced by the parser. Only the
    /// fixture can catch that.
    fn assert_declared_kinds_are_observed(&self, tree: &tree_sitter::Tree) {
        let observed = observed_kinds(tree.root_node());
        let optional: HashSet<&str> = self.spec.optional_kinds.iter().copied().collect();

        let missing: Vec<&str> = self
            .spec
            .kinds
            .all()
            .filter(|kind| !optional.contains(kind) && !observed.contains(*kind))
            .collect();

        assert!(
            missing.is_empty(),
            "[{}] declared kinds never produced by the fixture: {missing:?}",
            self.spec.id
        );
    }

    fn assert_either_of_groups_are_live(&self, tree: &tree_sitter::Tree) {
        let observed = observed_kinds(tree.root_node());

        for group in self.either_of {
            assert!(
                group.iter().any(|kind| observed.contains(*kind)),
                "[{}] none of {group:?} matched; the grammar now uses something else",
                self.spec.id
            );
        }
    }

    /// tree-sitter aliases can make `kind_id` differ from the symbol looked up by name, which
    /// would make the whole id-indexed table miss. If this ever fires the fix is a string
    /// compare for the affected kind, but the table should not rely on hope.
    fn assert_no_alias_drift(&self, tree: &tree_sitter::Tree) {
        let declared: HashSet<&str> = self.spec.kinds.all().collect();
        let mut drifted = Vec::new();
        collect_drift(tree.root_node(), &self.language, &declared, &mut drifted);
        drifted.sort_unstable();
        drifted.dedup();

        assert!(
            drifted.is_empty(),
            "[{}] kind_id does not match id_for_node_kind for: {drifted:?}",
            self.spec.id
        );
    }

    /// The walker assumes every child under an `if`'s alternative field is an else or else-if
    /// clause. A grammar can break that without renaming anything.
    fn assert_if_alternatives_are_else_kinds(&self, tree: &tree_sitter::Tree) {
        let language = self
            .spec
            .compile(self.language.clone())
            .expect("spec compiles");
        let Some(alternative) = language.fields.if_alternative else {
            return;
        };

        let mut offenders = Vec::new();
        visit(tree.root_node(), &mut |node| {
            collect_odd_alternatives(node, &language, alternative, &mut offenders);
        });
        offenders.sort_unstable();
        offenders.dedup();

        assert!(
            offenders.is_empty(),
            "[{}] unexpected kinds under an if alternative: {offenders:?}",
            self.spec.id
        );
    }
}

fn observed_kinds(root: Node<'_>) -> HashSet<String> {
    let mut found = HashSet::new();
    visit(root, &mut |node| {
        found.insert(node.kind().to_string());
    });
    found
}

fn collect_drift(
    node: Node<'_>,
    language: &TsLanguage,
    declared: &HashSet<&str>,
    drifted: &mut Vec<String>,
) {
    let kind = node.kind();
    if declared.contains(kind) && language.id_for_node_kind(kind, node.is_named()) != node.kind_id()
    {
        drifted.push(kind.to_string());
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_drift(child, language, declared, drifted);
    }
}

fn visit(node: Node<'_>, f: &mut impl FnMut(Node<'_>)) {
    f(node);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit(child, f);
    }
}

fn collect_odd_alternatives(
    node: tree_sitter::Node<'_>,
    language: &bonsai_core::Language,
    alternative: std::num::NonZeroU16,
    offenders: &mut Vec<String>,
) {
    if language.role(node) != Role::If {
        return;
    }
    let mut cursor = node.walk();
    let odd = node
        .children_by_field_id(alternative, &mut cursor)
        .filter(|alt| !matches!(language.role(*alt), Role::Else | Role::ElseIf))
        .map(|alt| alt.kind().to_string());
    offenders.extend(odd);
}
