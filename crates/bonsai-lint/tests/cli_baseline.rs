//! A baseline is an accepted debt ledger: what it records, honours and calls stale.

use std::fs;

mod common;

use common::{code, stderr, stdout, Project, BUSY_PHP, BUSY_PHP_TOO, CALM_PHP};
#[test]
fn stale_baseline_entries_are_reported_only_by_a_scan_that_covers_the_domain() {
    let project = Project::new();
    project
        .file("src/a.php", BUSY_PHP)
        .file("lib/b.php", BUSY_PHP_TOO);
    assert_eq!(
        code(&project.run(&["--over", "1", "--write-baseline", "."])),
        0
    );

    let stale = "matched nothing";
    let buffer = project.run_with_stdin(
        &["--over", "1", "--stdin", "--stdin-path", "src/a.php"],
        BUSY_PHP,
    );
    assert!(!stderr(&buffer).contains(stale), "{}", stderr(&buffer));

    let whole = project.run(&["--over", "1", "."]);
    assert_eq!(code(&whole), 0, "{}", stderr(&whole));
    assert!(!stderr(&whole).contains(stale), "{}", stderr(&whole));

    fs::remove_file(project.root.join("lib/b.php")).expect("remove");

    let partial = project.run(&["--over", "1", "src"]);
    assert!(!stderr(&partial).contains(stale), "{}", stderr(&partial));

    let whole = project.run(&["--over", "1", "."]);
    assert!(stderr(&whole).contains(stale), "{}", stderr(&whole));
}

/// Pre-commit hooks pass changed files one by one, and each of them must see the baseline the
/// whole-project run wrote.
#[test]
fn a_single_file_scan_shares_the_project_baseline_without_a_config() {
    let project = Project::new();
    project.file("src/a.php", BUSY_PHP);
    assert_eq!(
        code(&project.run(&["--over", "1", "--write-baseline", "."])),
        0
    );
    assert!(project.root.join(".bonsai-lint-baseline.json").is_file());

    let output = project.run(&["--over", "1", "src/a.php"]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(!project.root.join("src/.bonsai-lint-baseline.json").exists());
}

#[test]
fn a_shared_baseline_keys_by_workspace_relative_path() {
    let project = Project::new();
    project
        .file("bonsai-lint.toml", "domains = [\"packages/*\"]\n")
        .file("packages/web/src/index.php", BUSY_PHP)
        .file("packages/api/src/index.php", BUSY_PHP_TOO);

    let written = project.run(&[
        "--over",
        "1",
        "--write-baseline",
        "--baseline",
        "shared.json",
        ".",
    ]);
    assert_eq!(code(&written), 0, "{}", stderr(&written));

    let shared = project.read("shared.json");
    assert!(shared.contains("packages/web/src/index.php"), "{shared}");
    assert!(shared.contains("packages/api/src/index.php"), "{shared}");

    let checked = project.run(&["--over", "1", "--baseline", "shared.json", "."]);
    assert_eq!(code(&checked), 0, "{}", stderr(&checked));
}

#[test]
fn write_baseline_says_so_when_there_is_nothing_to_record() {
    let project = Project::new();
    project.file("src/calm.php", CALM_PHP);

    let output = project.run(&["--write-baseline", "."]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(
        stdout(&output).contains("no baseline written"),
        "{}",
        stdout(&output)
    );
    assert!(!project.root.join(".bonsai-lint-baseline.json").exists());
}
