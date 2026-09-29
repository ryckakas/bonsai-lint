//! A baseline is an accepted debt ledger: what it records, honours, and names once the code has
//! outgrown it.

use std::fs;

mod common;

use common::{BUSY_PHP, BUSY_PHP_TOO, CALM_PHP, Project, code, nested_php, report, stderr, stdout};
use serde_json::json;
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
    assert!(
        stderr(&whole)
            .contains("root: lib/b.php: busier is baselined at 3 but matched nothing in this scan"),
        "{}",
        stderr(&whole)
    );
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

const OUTGROWN: [&str; 4] = [
    "root: src/a.php: busy is baselined at 28 but scores 21",
    "root: src/b.php: gone is baselined at 21 but matched nothing in this scan",
    "root: src/c.php: hushed is baselined at 21 but is suppressed",
    "root: src/d.php: easy is baselined at 21 but scores 15, not over its threshold of 15",
];

fn reasoned(source: &str) -> String {
    source.replacen(
        "<?php\n",
        "<?php\n// bonsai-lint-ignore: kept as the spec reads\n",
        1,
    )
}

/// Four units a baseline records, then every way the code can outgrow it: one improves but stays
/// over the threshold, one is deleted, one gains a reasoned marker, one drops to the threshold.
fn outgrown_project() -> Project {
    let project = Project::new();
    project
        .file("src/a.php", &nested_php("busy", 7))
        .file("src/b.php", &nested_php("gone", 6))
        .file("src/c.php", &nested_php("hushed", 6))
        .file("src/d.php", &nested_php("easy", 6));
    assert_eq!(code(&project.run(&["--write-baseline", "."])), 0);

    project
        .file("src/a.php", &nested_php("busy", 6))
        .file("src/c.php", &reasoned(&nested_php("hushed", 6)))
        .file("src/d.php", &nested_php("easy", 5));
    fs::remove_file(project.root.join("src/b.php")).expect("remove");
    project
}

#[test]
fn every_entry_the_code_has_outgrown_is_named_without_failing_the_run() {
    let project = outgrown_project();

    let output = project.run(&["."]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    for line in OUTGROWN {
        assert!(stderr(&output).contains(line), "{}", stderr(&output));
    }
}

#[test]
fn a_strict_baseline_fails_until_it_is_rewritten() {
    let project = outgrown_project();

    let strict = project.run(&["--strict-baseline", "."]);
    assert_eq!(code(&strict), 1, "{}", stderr(&strict));
    assert!(
        stderr(&strict).contains(
            "4 entr(ies) of a strict baseline looser than the code; tighten them with \
             --write-baseline over the whole workspace, with this run's other flags"
        ),
        "{}",
        stderr(&strict)
    );

    assert_eq!(code(&project.run(&["--write-baseline", "."])), 0);
    let tightened = project.run(&["--strict-baseline", "."]);
    assert_eq!(code(&tightened), 0, "{}", stderr(&tightened));
    assert_eq!(stderr(&tightened), "");
}

#[test]
fn the_config_key_is_as_strict_as_the_flag() {
    let project = outgrown_project();
    project.file("bonsai-lint.toml", "strict-baseline = true\n");

    let output = project.run(&["."]);

    assert_eq!(code(&output), 1, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("4 entr(ies) of a strict baseline looser than the code"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn every_reason_is_listed_in_the_json_report() {
    let project = outgrown_project();

    let output = project.run(&["--format", "json", "."]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(
        report(&output)["loose_entries"],
        json!([
            {"domain": "root", "path": "src/a.php", "unit": "busy", "recorded": 28,
             "reason": "lower", "score": 21, "threshold": null, "strict": false},
            {"domain": "root", "path": "src/b.php", "unit": "gone", "recorded": 21,
             "reason": "unmatched", "score": null, "threshold": null, "strict": false},
            {"domain": "root", "path": "src/c.php", "unit": "hushed", "recorded": 21,
             "reason": "suppressed", "score": null, "threshold": null, "strict": false},
            {"domain": "root", "path": "src/d.php", "unit": "easy", "recorded": 21,
             "reason": "not_over_threshold", "score": 15, "threshold": 15, "strict": false},
        ])
    );
}

/// `breaches` stays the count of units over the threshold, so a strict failure has to be readable
/// from the entries themselves.
#[test]
fn a_strict_json_run_fails_with_no_breach_and_marks_the_entries_strict() {
    let project = outgrown_project();

    let output = project.run(&["--strict-baseline", "--format", "json", "."]);

    assert_eq!(code(&output), 1, "{}", stderr(&output));
    let report = report(&output);
    assert_eq!(report["breaches"], 0);
    let entries = report["loose_entries"].as_array().expect("an array");
    assert_eq!(entries.len(), 4);
    assert!(
        entries.iter().all(|entry| entry["strict"] == true),
        "{entries:?}"
    );
}

#[test]
fn a_clean_run_and_a_partial_scan_list_no_loose_entries() {
    let project = Project::new();
    project.file("src/a.php", &nested_php("busy", 6));
    assert_eq!(code(&project.run(&["--write-baseline", "."])), 0);

    let clean = project.run(&["--format", "json", "."]);
    assert_eq!(report(&clean)["loose_entries"], json!([]));

    project.file("src/a.php", &nested_php("busy", 5));
    let partial = project.run(&["--format", "json", "src/a.php"]);
    assert_eq!(report(&partial)["loose_entries"], json!([]));
}

/// A unit with work still pending must be fixed before the baseline is rewritten, or the rewrite
/// would accept it along with the tightened entries.
#[test]
fn a_breach_is_to_be_fixed_before_the_baseline_is_tightened() {
    let project = outgrown_project();
    project.file("src/e.php", &nested_php("fresh", 6));

    let output = project.run(&["--strict-baseline", "."]);

    assert_eq!(code(&output), 1, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("1 unit(s) over the threshold"),
        "{}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("; once nothing else fails, tighten them with --write-baseline"),
        "{}",
        stderr(&output)
    );
}

/// A pre-commit hook or an editor sees a few files, never the entries whose units are gone, so
/// it passes; an explicit flag says why it had no effect.
#[test]
fn a_scan_that_cannot_judge_the_baseline_never_fails_it() {
    let project = outgrown_project();
    let note = "--strict-baseline: 1 baseline(s) not judged";

    let buffer = project.run_with_stdin(
        &["--strict-baseline", "--stdin", "--stdin-path", "src/a.php"],
        &nested_php("busy", 6),
    );
    assert_eq!(code(&buffer), 0, "{}", stderr(&buffer));
    assert!(stderr(&buffer).contains(note), "{}", stderr(&buffer));

    for args in [
        &["--strict-baseline", "src/a.php", "src/d.php"][..],
        &["--strict-baseline", "--domain", "root", "."][..],
        &["--strict-baseline", "--lang", "php", "."][..],
    ] {
        let output = project.run(args);
        assert_eq!(code(&output), 0, "{args:?}: {}", stderr(&output));
        assert!(
            stderr(&output).contains(note),
            "{args:?}: {}",
            stderr(&output)
        );
        assert!(
            !stderr(&output).contains("baselined at"),
            "{args:?}: {}",
            stderr(&output)
        );
    }

    project.file("bonsai-lint.toml", "strict-baseline = true\n");
    let hook = project.run(&["src/a.php"]);
    assert_eq!(code(&hook), 0, "{}", stderr(&hook));
    assert!(!stderr(&hook).contains(note), "{}", stderr(&hook));
}

#[test]
fn a_raised_threshold_leaves_the_entry_unneeded() {
    let project = Project::new();
    project.file("src/a.php", &nested_php("busy", 6));
    assert_eq!(code(&project.run(&["--write-baseline", "."])), 0);

    let output = project.run(&["--over", "30", "."]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(
        stderr(&output).contains(
            "root: src/a.php: busy is baselined at 21 but scores 21, not over its threshold of 30"
        ),
        "{}",
        stderr(&output)
    );
}

/// A shared baseline belongs to no one domain, and its keys run from the workspace root, so the
/// check must come after the keys are rewritten or every entry would read as unmatched.
#[test]
fn a_shared_baseline_is_judged_by_workspace_relative_keys() {
    let project = Project::new();
    project
        .file("bonsai-lint.toml", "domains = [\"packages/*\"]\n")
        .file("packages/web/a.php", &nested_php("busy", 7))
        .file("packages/api/a.php", &nested_php("steady", 6));
    let shared = ["--baseline", "shared.json"];
    assert_eq!(
        code(&project.run(&[&shared[..], &["--write-baseline", "."]].concat())),
        0
    );

    let fresh = project.run(&[&shared[..], &["."]].concat());
    assert_eq!(code(&fresh), 0, "{}", stderr(&fresh));
    assert!(
        !stderr(&fresh).contains("baselined at"),
        "{}",
        stderr(&fresh)
    );

    project.file("packages/web/a.php", &nested_php("busy", 6));
    let output = project.run(&[&shared[..], &["--format", "json", "."]].concat());

    assert!(
        stderr(&output)
            .contains("baseline: packages/web/a.php: busy is baselined at 28 but scores 21"),
        "{}",
        stderr(&output)
    );
    assert_eq!(report(&output)["loose_entries"][0]["domain"], json!(null));
    assert_eq!(
        report(&output)["loose_entries"][0]["path"],
        "packages/web/a.php"
    );
}

#[test]
fn writing_a_baseline_under_strict_succeeds() {
    let project = outgrown_project();

    let output = project.run(&["--write-baseline", "--strict-baseline", "."]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
}
