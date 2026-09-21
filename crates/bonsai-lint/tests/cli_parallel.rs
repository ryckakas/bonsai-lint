//! A parallel scan must print exactly what a serial one prints.

use std::fmt::Write as _;
use std::fs;

mod common;

use common::{
    assert_matches_serial, code, over_threshold, report, stderr, stdout, Project, BUSY_PHP,
    CALM_PHP, CALM_TS,
};
/// Baits completion-order bugs: a slow first file so the worker holding index 0 finishes last,
/// ties that `rank` cannot break, and a warning with a position to be wrong about.
fn racy_project() -> Project {
    let project = Project::new();
    project.file("bonsai-lint.toml", "exclude = [\"skipped/**\"]\n");

    let mut heavy = String::from("<?php\n");
    for index in 0..4000 {
        writeln!(
            heavy,
            "function h{index}($a) {{ if ($a) {{ return 1; }} return 0; }}"
        )
        .expect("string write");
    }
    project.file("aaa_heavy.php", &heavy);

    let mut deep = String::from("<?php\n$x = 'a'");
    for _ in 0..30_000 {
        deep.push_str(" . 'a'");
    }
    deep.push_str(";\n");
    project.file("deep.php", &deep);

    for index in 0..40 {
        project.file(&format!("src/tie{index}.php"), &over_threshold("tie"));
        project.file(&format!("src/calm{index}.ts"), CALM_TS);
    }

    let unit = "function ($a) { if ($a) { if ($a) { if ($a) { if ($a) { if ($a) { if ($a) { return 1; } } } } } } }";
    project.file(
        "src/same_line.php",
        &format!("<?php\n$a = {unit}; $b = {unit};\n"),
    );
    project.file("src/types.d.ts", "export declare function f(): void;\n");
    project.file("skipped/ignored.php", BUSY_PHP);

    let mut legacy = over_threshold("legacy").into_bytes();
    legacy.extend_from_slice(b"// caf\xE9\n");
    fs::write(project.root.join("src/legacy.php"), legacy).expect("write");

    project
}

#[test]
fn a_parallel_scan_prints_exactly_what_a_serial_one_prints() {
    let project = racy_project();

    for format in ["text", "json"] {
        let serial = project.run(&["--all", "--format", format, "--jobs", "1", "."]);
        for jobs in ["2", "3", "8", "16", "0", "4096"] {
            assert_matches_serial(&project, format, jobs, &serial);
        }
    }
}

#[test]
fn a_parallel_scan_finds_the_same_breaches_a_serial_one_finds() {
    let project = racy_project();

    let serial = project.run(&["--format", "json", "--jobs", "1", "."]);
    assert_eq!(code(&serial), 1, "{}", stderr(&serial));
    assert!(
        report(&serial)["breaches"].as_u64().expect("breaches") > 40,
        "the fixture must actually breach the threshold"
    );
    assert!(
        stderr(&serial).contains("not valid UTF-8"),
        "the fixture must exercise a warning: {}",
        stderr(&serial)
    );

    let parallel = project.run(&["--format", "json", "--jobs", "8", "."]);
    assert_eq!(stdout(&parallel), stdout(&serial));
    assert_eq!(stderr(&parallel), stderr(&serial));
}

#[test]
fn a_baseline_is_the_same_file_however_many_workers_wrote_it() {
    let project = racy_project();
    project.file("src/twice.php", &over_threshold("same"));

    let serial = project.run(&["--write-baseline", "--jobs", "1", "."]);
    assert_eq!(code(&serial), 0, "{}", stderr(&serial));
    let expected = project.read(".bonsai-lint-baseline.json");

    for jobs in ["2", "8", "16"] {
        let parallel = project.run(&["--write-baseline", "--jobs", jobs, "."]);
        assert_eq!(code(&parallel), 0, "{}", stderr(&parallel));
        assert_eq!(
            project.read(".bonsai-lint-baseline.json"),
            expected,
            "jobs={jobs}"
        );
    }
}

#[test]
fn jobs_is_accepted_alongside_stdin() {
    let project = Project::new();
    project.file("src/a.php", CALM_PHP);

    let output = project.run_with_stdin(
        &["--stdin", "--stdin-path", "src/a.php", "--jobs", "4"],
        BUSY_PHP,
    );

    assert_eq!(code(&output), 0, "{}", stderr(&output));
}
