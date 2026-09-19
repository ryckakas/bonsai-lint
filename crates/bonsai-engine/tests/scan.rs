//! The scan driver decides which files are read and how; these are the rules a user sees as
//! "why was this file (not) reported".

use std::fs;
use std::path::{Path, PathBuf};

use bonsai_engine::config;
use bonsai_engine::scan::{display_path, ScanOutcome};
use bonsai_engine::Scanner;

const KNOWN: &[&str] = &["php", "typescript"];
const PHP_UNIT: &str = "<?php\nfunction f() { return 1; }\n";
const TS_UNIT: &str = "function f() { return 1; }\n";

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
    let workspace = config::discover(root, KNOWN).expect("discovery");
    let paths: Vec<PathBuf> = paths.iter().map(|path| root.join(path)).collect();
    Scanner::new().scan(&paths, &workspace, languages)
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
