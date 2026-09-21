//! The scan driver decides which files are read and how; these are the rules a user sees as
//! "why was this file (not) reported".

use std::fmt::Write as _;
use std::fs;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use bonsai_engine::config;
use bonsai_engine::scan::{display_path, ScanOutcome};
use bonsai_engine::Scanner;

const KNOWN: &[&str] = &["php", "typescript", "vue"];
const PHP_UNIT: &str = "<?php\nfunction f() { return 1; }\n";
const TS_UNIT: &str = "function f() { return 1; }\n";
const VUE_UNIT: &str =
    "<template><p v-if=\"a\">x</p></template>\n<script setup lang=\"ts\">\nfunction f() { return 1 }\n</script>\n";

fn project() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().canonicalize().expect("canonical temp dir");
    (dir, root)
}

fn write(root: &Path, relative: &str, content: &[u8]) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("has parent")).expect("mkdir");
    fs::write(path, content).expect("write");
}

fn scan(root: &Path, paths: &[&str], languages: Option<&[String]>) -> ScanOutcome {
    scan_with_jobs(root, paths, languages, None)
}

fn scan_with_jobs(
    root: &Path,
    paths: &[&str],
    languages: Option<&[String]>,
    jobs: Option<usize>,
) -> ScanOutcome {
    let workspace = config::discover(root, KNOWN).expect("discovery");
    let paths: Vec<PathBuf> = paths.iter().map(|path| root.join(path)).collect();
    Scanner::new()
        .with_jobs(jobs.and_then(NonZeroUsize::new))
        .scan(&paths, &workspace, languages)
}

fn shape(outcome: &ScanOutcome) -> String {
    format!("{:?}", outcome.located)
}

#[test]
fn overlapping_scan_paths_report_each_file_once() {
    let (_dir, root) = project();
    write(&root, "src/a.php", PHP_UNIT.as_bytes());

    let outcome = scan(&root, &["src", "src/a.php"], None);

    assert_eq!(outcome.stats.files, 1);
    assert_eq!(outcome.located.len(), 1);
}

#[test]
fn invalid_utf8_is_decoded_leniently_with_a_warning() {
    let (_dir, root) = project();
    write(
        &root,
        "src/legacy.php",
        b"<?php\nfunction f() { return \"caf\xE9\"; }\n",
    );

    let outcome = scan(&root, &["src"], None);

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.warnings.len(), 1, "{:?}", outcome.warnings);
    assert!(outcome.warnings[0].contains("not valid UTF-8"));
    assert_eq!(outcome.located.len(), 1);
}

#[test]
fn excluded_files_are_neither_read_nor_counted() {
    let (_dir, root) = project();
    write(&root, "bonsai-lint.toml", b"exclude = [\"vendor/**\"]\n");
    write(&root, "vendor/lib/x.php", PHP_UNIT.as_bytes());
    write(&root, "src/a.php", PHP_UNIT.as_bytes());

    let outcome = scan(&root, &["."], None);

    assert_eq!(outcome.stats.files, 1);
    assert!(outcome
        .located
        .iter()
        .all(|item| !item.path.to_string_lossy().contains("vendor")));
}

/// `.js` and `.tsx` are parsed by a different grammar than `.ts`, but they are one language to
/// the user, so one id must select all of them.
#[test]
fn the_language_filter_selects_by_language_not_by_grammar() {
    let (_dir, root) = project();
    write(&root, "a.ts", TS_UNIT.as_bytes());
    write(&root, "b.js", TS_UNIT.as_bytes());
    write(&root, "c.tsx", TS_UNIT.as_bytes());
    write(&root, "d.php", PHP_UNIT.as_bytes());

    let typescript = scan(&root, &["."], Some(&["typescript".to_string()]));
    assert_eq!(typescript.stats.files, 3);

    let php = scan(&root, &["."], Some(&["php".to_string()]));
    assert_eq!(php.stats.files, 1);
}

#[test]
fn unreadable_paths_are_errors_that_taint_the_scan() {
    let (_dir, root) = project();

    let outcome = scan(&root, &["missing"], None);

    assert_eq!(outcome.stats.errors, 1);
    assert_eq!(outcome.stats.files, 0);
}

#[test]
fn reported_paths_use_one_separator_on_every_platform() {
    assert_eq!(
        display_path(Path::new("./src/a.php")),
        Path::new("src/a.php")
    );
    assert_eq!(
        display_path(Path::new(r"packages\web\src\a.ts")).to_string_lossy(),
        "packages/web/src/a.ts"
    );
    assert_eq!(display_path(Path::new(".")), Path::new("."));
}

#[test]
fn every_file_is_counted_once_however_many_workers_run() {
    let (_dir, root) = project();
    write(&root, "bonsai-lint.toml", b"domains = [\"packages/*\"]\n");
    for index in 0..32 {
        let mut php = String::new();
        writeln!(
            php,
            "<?php\nfunction a{index}($x) {{ if ($x) {{ return 1; }} return 0; }}"
        )
        .expect("string write");
        write(&root, &format!("packages/api/f{index}.php"), php.as_bytes());
        write(
            &root,
            &format!("packages/web/f{index}.ts"),
            TS_UNIT.as_bytes(),
        );
    }

    let serial = scan_with_jobs(&root, &["."], None, Some(1));
    assert_eq!(serial.stats.files, 64);
    assert_eq!(serial.stats.domains.len(), 2);
    assert!(serial.errors.is_empty(), "{:?}", serial.errors);

    for jobs in [2, 4, 8, 16] {
        let parallel = scan_with_jobs(&root, &["."], None, Some(jobs));
        assert_eq!(parallel.stats.files, serial.stats.files, "jobs={jobs}");
        assert_eq!(parallel.stats.errors, serial.stats.errors, "jobs={jobs}");
        assert_eq!(parallel.stats.domains, serial.stats.domains, "jobs={jobs}");
        assert_eq!(shape(&parallel), shape(&serial), "jobs={jobs}");
    }
}

#[test]
fn warnings_come_back_in_walk_order_whatever_the_workers_do() {
    let (_dir, root) = project();
    for index in 0..16 {
        write(&root, &format!("src/f{index}.php"), PHP_UNIT.as_bytes());
    }
    for index in 0..8 {
        write(
            &root,
            &format!("src/legacy{index}.php"),
            b"<?php\nfunction f() { return \"caf\xE9\"; }\n",
        );
    }

    let serial = scan_with_jobs(&root, &["src"], None, Some(1));
    assert_eq!(serial.warnings.len(), 8, "{:?}", serial.warnings);

    for jobs in [2, 8, 16] {
        let parallel = scan_with_jobs(&root, &["src"], None, Some(jobs));
        assert_eq!(parallel.warnings, serial.warnings, "jobs={jobs}");
    }
}

/// The only test that can prove the engine parses on a stack it sized itself: it runs on a
/// libtest thread, so a worker spawned without one would take the process down here.
#[test]
fn deeply_nested_sources_are_parsed_on_a_stack_sized_for_them() {
    let (_dir, root) = project();
    let mut source = String::from("<?php\n$x = 'a'");
    for _ in 0..30_000 {
        source.push_str(" . 'a'");
    }
    source.push_str(";\n");
    write(&root, "deep.php", source.as_bytes());

    let outcome = scan(&root, &["deep.php"], None);

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.stats.files, 1);
}

#[test]
fn a_vue_component_is_discovered_and_scored_as_vue() {
    let (_dir, root) = project();
    write(&root, "src/Panel.vue", VUE_UNIT.as_bytes());

    let outcome = scan(&root, &["."], None);

    let languages: Vec<_> = outcome
        .located
        .iter()
        .map(|located| located.finding.language)
        .collect();
    assert_eq!(languages, vec!["vue"]);
}

/// `.vue` and `.ts` are separate ids, so a filter naming one must not pick up the other.
#[test]
fn the_language_filter_separates_vue_from_typescript() {
    let (_dir, root) = project();
    write(&root, "src/Panel.vue", VUE_UNIT.as_bytes());
    write(&root, "src/plain.ts", TS_UNIT.as_bytes());

    let only_vue = scan(&root, &["."], Some(&["vue".to_string()]));
    assert_eq!(only_vue.stats.files, 1);
    assert!(only_vue
        .located
        .iter()
        .all(|located| located.finding.language == "vue"));

    let only_ts = scan(&root, &["."], Some(&["typescript".to_string()]));
    assert_eq!(only_ts.stats.files, 1);
    assert!(only_ts
        .located
        .iter()
        .all(|located| located.finding.language == "typescript"));
}

#[test]
fn a_vue_component_without_a_script_block_is_read_without_error() {
    let (_dir, root) = project();
    write(
        &root,
        "src/Static.vue",
        b"<template><p>only</p></template>\n",
    );

    let outcome = scan(&root, &["."], None);

    assert_eq!(outcome.stats.errors, 0);
    assert!(outcome.warnings.is_empty());
    assert!(outcome.located.is_empty());
}
