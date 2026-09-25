//! Java end to end: scoring, overload keys, configuration, generated files and baselines.

mod common;

use common::{
    code, java_overloads, paths, report, stderr, stdout, Project, BUSY_JAVA, BUSY_TS, CALM_JAVA,
    JAVA_GENERATED_HEADER,
};

#[test]
fn a_java_method_is_scored_and_reported_below_its_annotations() {
    let project = Project::new();
    project.file("src/main/java/p/Busy.java", BUSY_JAVA);

    let output = project.run(&["--over", "0", "--format", "json", "."]);

    let finding = &report(&output)["findings"][0];
    assert_eq!(finding["name"], "Busy::busy(boolean, boolean)");
    assert_eq!(finding["language"], "java");
    assert_eq!(finding["line"], 5);
    assert_eq!(finding["score"], 3);
    assert_eq!(code(&output), 1);
}

#[test]
fn the_language_filter_selects_java_alone() {
    let project = Project::new();
    project
        .file("src/Calm.java", CALM_JAVA)
        .file("web/plain.ts", BUSY_TS);

    let listing = stdout(&project.run(&["--all", "--lang", "java", "."]));

    assert!(listing.contains("Calm.java"), "{listing}");
    assert!(!listing.contains("plain.ts"), "{listing}");
}

#[test]
fn a_java_override_applies_only_to_java() {
    let project = Project::new();
    project
        .file("src/Busy.java", BUSY_JAVA)
        .file("web/plain.ts", BUSY_TS);

    let output = project.run(&["--over", "java=0", "--format", "json", "."]);

    assert_eq!(paths(&output), vec!["src/Busy.java"]);
}

#[test]
fn a_java_threshold_section_applies_only_to_java() {
    let project = Project::new();
    project.file(
        "bonsai-lint.toml",
        "threshold = 0\n\n[java]\nthreshold = 99\n",
    );
    project
        .file("src/Busy.java", BUSY_JAVA)
        .file("web/plain.ts", BUSY_TS);

    let output = project.run(&["--format", "json", "."]);

    assert_eq!(paths(&output), vec!["web/plain.ts"]);
}

#[test]
fn an_unsaved_java_buffer_is_scored_through_stdin() {
    let project = Project::new();

    let output = project.run_with_stdin(
        &[
            "--over",
            "0",
            "--format",
            "json",
            "--stdin",
            "--stdin-path",
            "src/Busy.java",
        ],
        BUSY_JAVA,
    );

    let finding = &report(&output)["findings"][0];
    assert_eq!(finding["language"], "java");
    assert_eq!(finding["line"], 5);
}

/// An editor opening a generated file must agree with CI, which never scores it.
#[test]
fn a_generated_java_buffer_reports_nothing_through_stdin() {
    let project = Project::new();

    let output = project.run_with_stdin(
        &[
            "--over",
            "0",
            "--format",
            "json",
            "--stdin",
            "--stdin-path",
            "src/Orders.java",
        ],
        &format!("{JAVA_GENERATED_HEADER}{BUSY_JAVA}"),
    );

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(report(&output)["findings"], serde_json::json!([]));
}

#[test]
fn a_generated_java_file_is_skipped_beside_handwritten_ones() {
    let project = Project::new();
    project
        .file(
            "src/Orders.java",
            &format!("{JAVA_GENERATED_HEADER}{BUSY_JAVA}"),
        )
        .file("src/Busy.java", BUSY_JAVA);

    let output = project.run(&["--over", "0", "--format", "json", "."]);

    assert_eq!(paths(&output), vec!["src/Busy.java"]);
}

/// A gate pointed only at generated code has checked nothing, so it must not report a clean run.
#[test]
fn a_path_holding_only_generated_java_is_not_a_clean_run() {
    let project = Project::new();
    project.file(
        "gen/Orders.java",
        &format!("{JAVA_GENERATED_HEADER}{BUSY_JAVA}"),
    );

    let output = project.run(&["gen"]);

    assert_eq!(code(&output), 1);
    assert!(
        stderr(&output).contains("no supported files found"),
        "{}",
        stderr(&output)
    );
}

/// Overloads share a name, so without the parameter types in the key the second would overwrite
/// the first in the baseline and leave it failing forever.
#[test]
fn overloads_are_baselined_apart_and_then_quiet() {
    let project = Project::new();
    project.file("bonsai-lint.toml", "threshold = 0\n");
    project.file("src/Orders.java", &java_overloads(3, 2));

    let written = project.run(&["--write-baseline", "."]);
    assert_eq!(code(&written), 0, "{}", stderr(&written));
    let baseline = project.read(".bonsai-lint-baseline.json");
    assert!(baseline.contains("Orders::process(boolean)"), "{baseline}");
    assert!(
        baseline.contains("Orders::process(boolean, boolean)"),
        "{baseline}"
    );

    let rerun = project.run(&["."]);
    assert_eq!(code(&rerun), 0, "{}", stdout(&rerun));
}

#[test]
fn a_worsened_overload_breaks_its_baseline_alone() {
    let project = Project::new();
    project.file("bonsai-lint.toml", "threshold = 0\n");
    project.file("src/Orders.java", &java_overloads(3, 2));
    project.run(&["--write-baseline", "."]);

    project.file("src/Orders.java", &java_overloads(3, 4));
    let rerun = project.run(&["."]);

    assert_eq!(code(&rerun), 1);
    let listing = stdout(&rerun);
    assert!(
        listing.contains("Orders::process(boolean, boolean)"),
        "{listing}"
    );
    assert!(!listing.contains("Orders::process(boolean)"), "{listing}");
}
