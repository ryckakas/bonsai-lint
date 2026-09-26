//! Python end to end: scoring, accessor keys, configuration, generated files and baselines.

mod common;

use common::{
    code, paths, python_accessors, report, stderr, stdout, Project, BUSY_PYTHON, BUSY_TS,
    CALM_PYTHON, PYTHON_GENERATED_HEADER,
};

fn names(output: &std::process::Output) -> Vec<String> {
    report(output)["findings"]
        .as_array()
        .expect("findings is an array")
        .iter()
        .map(|finding| finding["name"].as_str().expect("name").to_string())
        .collect()
}

#[test]
fn a_python_method_is_scored_and_reported_below_its_decorators() {
    let project = Project::new();
    project.file("app/busy.py", BUSY_PYTHON);

    let output = project.run(&["--over", "0", "--format", "json", "."]);

    let finding = &report(&output)["findings"][0];
    assert_eq!(finding["name"], "Busy::busy");
    assert_eq!(finding["language"], "python");
    assert_eq!(finding["line"], 3);
    assert_eq!(finding["score"], 3);
    assert_eq!(code(&output), 1);
}

#[test]
fn the_language_filter_selects_python_alone() {
    let project = Project::new();
    project
        .file("app/calm.py", CALM_PYTHON)
        .file("web/plain.ts", BUSY_TS);

    let listing = stdout(&project.run(&["--all", "--lang", "python", "."]));

    assert!(listing.contains("calm.py"), "{listing}");
    assert!(!listing.contains("plain.ts"), "{listing}");
}

/// A Windows `.pyw` script is Python; a `.pyi` stub declares types and holds no logic.
#[test]
fn a_pyw_script_is_scored_and_a_pyi_stub_is_not_scanned() {
    let project = Project::new();
    project
        .file("app/busy.py", BUSY_PYTHON)
        .file("app/gui.pyw", BUSY_PYTHON)
        .file("app/busy.pyi", BUSY_PYTHON);

    let output = project.run(&["--over", "0", "--format", "json", "."]);

    assert_eq!(paths(&output), vec!["app/busy.py", "app/gui.pyw"]);
}

#[test]
fn a_python_override_applies_only_to_python() {
    let project = Project::new();
    project
        .file("app/busy.py", BUSY_PYTHON)
        .file("web/plain.ts", BUSY_TS);

    let output = project.run(&["--over", "python=0", "--format", "json", "."]);

    assert_eq!(paths(&output), vec!["app/busy.py"]);
}

#[test]
fn a_python_threshold_section_applies_only_to_python() {
    let project = Project::new();
    project.file(
        "bonsai-lint.toml",
        "threshold = 0\n\n[python]\nthreshold = 99\n",
    );
    project
        .file("app/busy.py", BUSY_PYTHON)
        .file("web/plain.ts", BUSY_TS);

    let output = project.run(&["--format", "json", "."]);

    assert_eq!(paths(&output), vec!["web/plain.ts"]);
}

#[test]
fn an_unsaved_python_buffer_is_scored_through_stdin() {
    let project = Project::new();

    let output = project.run_with_stdin(
        &[
            "--over",
            "0",
            "--format",
            "json",
            "--stdin",
            "--stdin-path",
            "app/busy.py",
        ],
        BUSY_PYTHON,
    );

    let finding = &report(&output)["findings"][0];
    assert_eq!(finding["language"], "python");
    assert_eq!(finding["line"], 3);
}

/// An editor opening a generated file must agree with CI, which never scores it.
#[test]
fn a_generated_python_buffer_reports_nothing_through_stdin() {
    let project = Project::new();

    let output = project.run_with_stdin(
        &[
            "--over",
            "0",
            "--format",
            "json",
            "--stdin",
            "--stdin-path",
            "app/orders_pb2.py",
        ],
        &format!("{PYTHON_GENERATED_HEADER}{BUSY_PYTHON}"),
    );

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(report(&output)["findings"], serde_json::json!([]));
}

#[test]
fn a_generated_python_file_is_skipped_beside_handwritten_ones() {
    let project = Project::new();
    project
        .file(
            "app/orders_pb2.py",
            &format!("{PYTHON_GENERATED_HEADER}{BUSY_PYTHON}"),
        )
        .file("app/busy.py", BUSY_PYTHON);

    let output = project.run(&["--over", "0", "--format", "json", "."]);

    assert_eq!(paths(&output), vec!["app/busy.py"]);
}

/// A gate pointed only at generated code has checked nothing, so it must not report a clean run.
#[test]
fn a_path_holding_only_generated_python_is_not_a_clean_run() {
    let project = Project::new();
    project.file(
        "gen/orders_pb2.py",
        &format!("{PYTHON_GENERATED_HEADER}{BUSY_PYTHON}"),
    );

    let output = project.run(&["gen"]);

    assert_eq!(code(&output), 1);
    assert!(
        stderr(&output).contains("no supported files found"),
        "{}",
        stderr(&output)
    );
}

/// A getter and its setter share a name, so without the accessor in the key the second would
/// overwrite the first in the baseline and leave it failing forever.
#[test]
fn property_accessors_are_baselined_apart_and_then_quiet() {
    let project = Project::new();
    project.file("bonsai-lint.toml", "threshold = 0\n");
    project.file("app/cart.py", &python_accessors(3, 2));

    let written = project.run(&["--write-baseline", "."]);
    assert_eq!(code(&written), 0, "{}", stderr(&written));
    let baseline = project.read(".bonsai-lint-baseline.json");
    assert!(baseline.contains("\"Cart::total\""), "{baseline}");
    assert!(baseline.contains("\"Cart::total.setter\""), "{baseline}");

    let rerun = project.run(&["."]);
    assert_eq!(code(&rerun), 0, "{}", stdout(&rerun));
}

#[test]
fn a_worsened_setter_breaks_its_baseline_alone() {
    let project = Project::new();
    project.file("bonsai-lint.toml", "threshold = 0\n");
    project.file("app/cart.py", &python_accessors(3, 2));
    project.run(&["--write-baseline", "."]);

    project.file("app/cart.py", &python_accessors(3, 4));
    let rerun = project.run(&["--format", "json", "."]);

    assert_eq!(code(&rerun), 1);
    assert_eq!(names(&rerun), vec!["Cart::total.setter"]);
}
