//! The unsaved-buffer path an editor drives, which must agree with a scan from disk.

mod common;

use common::{
    code, paths, ranked_scores, report, scores, stderr, Project, BUSY_PHP, BUSY_TS, CALM_PHP,
};
#[test]
fn stdin_respects_the_workspace_excludes() {
    let project = Project::new();
    project
        .file("bonsai-lint.toml", "exclude = [\"vendor/**\"]\n")
        .file("vendor/x.php", BUSY_PHP);

    let output = project.run_with_stdin(
        &[
            "--format",
            "json",
            "--over",
            "1",
            "--stdin",
            "--stdin-path",
            "vendor/x.php",
        ],
        BUSY_PHP,
    );

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(scores(&output).is_empty());
}

#[test]
fn stdin_scores_the_buffer_not_the_file_on_disk() {
    let project = Project::new();
    project.file("src/a.php", CALM_PHP);

    let output = project.run_with_stdin(
        &[
            "--format",
            "json",
            "--over",
            "1",
            "--stdin",
            "--stdin-path",
            "src/a.php",
        ],
        BUSY_PHP,
    );

    assert_eq!(code(&output), 1);
    assert_eq!(scores(&output), vec![3]);
}

#[test]
fn stdin_resolves_the_same_workspace_as_a_file_scan() {
    let project = Project::new();
    project
        .file(
            "bonsai-lint.toml",
            "domains = [\"apps/*\"]\nthreshold = 15\n",
        )
        .file("apps/wallet/bonsai-lint.toml", "threshold = 2\n")
        .file("apps/wallet/src/a.ts", BUSY_TS);

    let from_disk = project.run(&["--format", "json", "apps/wallet/src/a.ts"]);
    let from_stdin = project.run_with_stdin(
        &[
            "--format",
            "json",
            "--stdin",
            "--stdin-path",
            "apps/wallet/src/a.ts",
        ],
        BUSY_TS,
    );

    assert_eq!(code(&from_disk), 1);
    assert_eq!(code(&from_stdin), 1, "{}", stderr(&from_stdin));
    assert_eq!(
        report(&from_disk)["thresholds"],
        report(&from_stdin)["thresholds"]
    );
    assert_eq!(paths(&from_disk), paths(&from_stdin));
}

#[test]
fn stdin_ranks_units_like_a_scan_from_disk() {
    let source = "<?php\nfunction calm() { return 1; }\nfunction busy($a, $b) {\n    if ($a) { if ($b) { return 1; } }\n    return 0;\n}\n";
    let project = Project::new();
    project.file("src/a.php", source);

    let from_disk = project.run(&["--all", "--format", "json", "src/a.php"]);
    let from_stdin = project.run_with_stdin(
        &[
            "--all",
            "--format",
            "json",
            "--stdin",
            "--stdin-path",
            "src/a.php",
        ],
        source,
    );

    assert_eq!(ranked_scores(&from_disk), vec![3, 0]);
    assert_eq!(ranked_scores(&from_stdin), ranked_scores(&from_disk));
}
