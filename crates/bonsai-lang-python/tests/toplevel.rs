//! Module code, class bodies and decorator arguments run on import and belong to the file
//! rather than to any function.

mod common;

use common::findings;

fn toplevel_score(source: &str) -> Option<u32> {
    findings(source)
        .into_iter()
        .find(|finding| finding.name == bonsai_core::TOPLEVEL_UNIT)
        .map(|finding| finding.score)
}

#[test]
fn module_level_logic_is_scored() {
    assert_eq!(toplevel_score("FLAG = a and b\n"), Some(1));
}

#[test]
fn the_main_guard_is_scored() {
    let source = "def main():\n    pass\n\nif __name__ == \"__main__\":\n    main()\n";
    assert_eq!(toplevel_score(source), Some(1));
}

#[test]
fn class_body_and_decorator_argument_logic_is_scored() {
    let class_body = "class C:\n    X = 1 if a else 2\n";
    assert_eq!(toplevel_score(class_body), Some(1));

    let decorator = "@app.route(\"/\", methods=M if a else N)\ndef index():\n    pass\n";
    assert_eq!(toplevel_score(decorator), Some(1));
}

#[test]
fn declared_units_are_not_counted_twice() {
    let source = "FLAG = a and b\nhandler = lambda e: 1 if e else 2\n\ndef f():\n    if c:\n        g()\n\nclass C:\n    def m(self):\n        if d:\n            g()\n";
    assert_eq!(toplevel_score(source), Some(1));
}

#[test]
fn a_file_without_module_level_logic_reports_nothing() {
    let source = "\"\"\"Docs.\"\"\"\n\nimport os\n\nNAMES = [x for x in os.listdir() if x]\n\ndef f():\n    if a:\n        g()\n";
    assert_eq!(toplevel_score(source), None);
}

#[test]
fn a_package_init_of_imports_reports_nothing() {
    let source =
        "\"\"\"Package.\"\"\"\nfrom .a import b\nfrom .c import d\n\n__all__ = [\"b\", \"d\"]\n";
    assert!(findings(source).is_empty());
}

#[test]
fn leading_blank_lines_do_not_move_the_toplevel_line() {
    let source = "\n\nFLAG = a and b\n";
    let line = findings(source)
        .into_iter()
        .find(|finding| finding.name == bonsai_core::TOPLEVEL_UNIT)
        .map(|finding| finding.line);
    assert_eq!(line, Some(1));
}
