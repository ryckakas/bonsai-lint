//! Go end to end: scoring, method keys, configuration, generated files and baselines.

mod common;

use common::{
    code, nested_go, paths, report, stderr, stdout, Project, BUSY_GO, BUSY_TS, CALM_GO,
    GENERATED_HEADER,
};

#[test]
fn a_go_function_is_scored_and_reported_at_its_line() {
    let project = Project::new();
    project.file("pkg/busy.go", BUSY_GO);

    let output = project.run(&["--over", "0", "--format", "json", "."]);

    let finding = &report(&output)["findings"][0];
    assert_eq!(finding["name"], "busy");
    assert_eq!(finding["language"], "go");
    assert_eq!(finding["line"], 3);
    assert_eq!(finding["score"], 3);
    assert_eq!(code(&output), 1);
}

#[test]
fn a_method_is_reported_under_its_receiver_type() {
    let project = Project::new();
    project.file("pkg/stack.go", &nested_go("Push", 2));

    let output = project.run(&["--over", "0", "--format", "json", "."]);

    assert_eq!(report(&output)["findings"][0]["name"], "Stack::Push");
}

#[test]
fn the_language_filter_selects_go_alone() {
    let project = Project::new();
    project
        .file("pkg/calm.go", CALM_GO)
        .file("web/plain.ts", BUSY_TS);

    let listing = stdout(&project.run(&["--all", "--lang", "go", "."]));

    assert!(listing.contains("calm.go"), "{listing}");
    assert!(!listing.contains("plain.ts"), "{listing}");
}

#[test]
fn a_go_override_applies_only_to_go() {
    let project = Project::new();
    project
        .file("pkg/busy.go", BUSY_GO)
        .file("web/plain.ts", BUSY_TS);

    let output = project.run(&["--over", "go=0", "--format", "json", "."]);

    assert_eq!(paths(&output), vec!["pkg/busy.go"]);
}

#[test]
fn a_go_threshold_section_applies_only_to_go() {
    let project = Project::new();
    project.file(
        "bonsai-lint.toml",
        "threshold = 0\n\n[go]\nthreshold = 99\n",
    );
    project
        .file("pkg/busy.go", BUSY_GO)
        .file("web/plain.ts", BUSY_TS);

    let output = project.run(&["--format", "json", "."]);

    assert_eq!(paths(&output), vec!["web/plain.ts"]);
}

#[test]
fn an_unsaved_go_buffer_is_scored_through_stdin() {
    let project = Project::new();

    let output = project.run_with_stdin(
        &[
            "--over",
            "0",
            "--format",
            "json",
            "--stdin",
            "--stdin-path",
            "pkg/busy.go",
        ],
        BUSY_GO,
    );

    let finding = &report(&output)["findings"][0];
    assert_eq!(finding["language"], "go");
    assert_eq!(finding["line"], 3);
}

/// An editor opening a generated file must agree with CI, which never scores it.
#[test]
fn a_generated_go_buffer_reports_nothing_through_stdin() {
    let project = Project::new();

    let output = project.run_with_stdin(
        &[
            "--over",
            "0",
            "--format",
            "json",
            "--stdin",
            "--stdin-path",
            "pkg/api.pb.go",
        ],
        &format!("{GENERATED_HEADER}{BUSY_GO}"),
    );

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(report(&output)["findings"], serde_json::json!([]));
}

#[test]
fn a_generated_go_file_is_skipped_beside_handwritten_ones() {
    let project = Project::new();
    project
        .file("pkg/api.pb.go", &format!("{GENERATED_HEADER}{BUSY_GO}"))
        .file("pkg/busy.go", BUSY_GO);

    let output = project.run(&["--over", "0", "--format", "json", "."]);

    assert_eq!(paths(&output), vec!["pkg/busy.go"]);
}

/// A gate pointed only at generated code has checked nothing, so it must not report a clean run.
#[test]
fn a_path_holding_only_generated_go_is_not_a_clean_run() {
    let project = Project::new();
    project.file("gen/api.pb.go", &format!("{GENERATED_HEADER}{BUSY_GO}"));

    let output = project.run(&["gen"]);

    assert_eq!(code(&output), 1);
    assert!(
        stderr(&output).contains("no supported files found"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_go_baseline_is_keyed_by_receiver_and_then_quiet() {
    let project = Project::new();
    project.file("bonsai-lint.toml", "threshold = 0\n");
    project.file("pkg/stack.go", &nested_go("Push", 3));

    let written = project.run(&["--write-baseline", "."]);
    assert_eq!(code(&written), 0, "{}", stderr(&written));
    assert!(project
        .read(".bonsai-lint-baseline.json")
        .contains("Stack::Push"));

    let rerun = project.run(&["."]);
    assert_eq!(code(&rerun), 0, "{}", stdout(&rerun));
}

#[test]
fn a_worsened_go_method_breaks_its_baseline() {
    let project = Project::new();
    project.file("bonsai-lint.toml", "threshold = 0\n");
    project.file("pkg/stack.go", &nested_go("Push", 3));
    project.run(&["--write-baseline", "."]);

    project.file("pkg/stack.go", &nested_go("Push", 5));
    let rerun = project.run(&["."]);

    assert_eq!(code(&rerun), 1);
    assert!(stdout(&rerun).contains("Stack::Push"), "{}", stdout(&rerun));
}
