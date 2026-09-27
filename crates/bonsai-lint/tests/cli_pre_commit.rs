//! The command line the pre-commit hook runs: staged files named one by one, in whatever order
//! pre-commit batches them, from the repository root. The arguments come from the hook
//! manifest itself, so these tests run exactly what users run.

use std::fs;
use std::path::Path;
use std::process::Output;

mod common;

use common::{code, stderr, stdout, Project, BUSY_GO, BUSY_PHP, BUSY_TS, GENERATED_HEADER};

fn manifest() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.pre-commit-hooks.yaml");
    fs::read_to_string(path).expect("the hook manifest sits at the repository root")
}

fn manifest_field(key: &str) -> String {
    let prefix = format!("  {key}: ");
    manifest()
        .lines()
        .find_map(|line| line.strip_prefix(&prefix).map(str::to_owned))
        .unwrap_or_else(|| panic!("the hook manifest has no `{key}`"))
}

fn hook(project: &Project, files: &[&str]) -> Output {
    let entry = manifest_field("entry");
    let mut words = entry.split_whitespace();
    assert_eq!(words.next(), Some("bonsai-lint"));
    let mut args: Vec<&str> = words.collect();
    args.extend_from_slice(files);
    project.run(&args)
}

/// autoupdate refuses a new rev that lacks a configured hook id, so renaming it breaks every user.
#[test]
fn the_hook_id_is_bonsai_lint() {
    assert!(
        manifest().lines().any(|line| line == "- id: bonsai-lint"),
        "{}",
        manifest()
    );
}

#[cfg(all(
    feature = "php",
    feature = "ts",
    feature = "vue",
    feature = "go",
    feature = "java",
    feature = "python"
))]
#[test]
fn the_hook_filters_on_exactly_the_registry_extensions() {
    let mut extensions: Vec<&str> = bonsai_engine::registry::descriptors()
        .iter()
        .flat_map(|descriptor| descriptor.extensions().iter().copied())
        .collect();
    extensions.sort_unstable();
    extensions.dedup();

    assert_eq!(
        manifest_field("files"),
        format!(r"\.({})$", extensions.join("|"))
    );
}

#[test]
fn a_batch_of_only_skipped_files_passes_with_allow_no_files() {
    let project = Project::new();
    project.file("src/vite-env.d.ts", BUSY_TS);
    project.file("gen/api.pb.go", &format!("{GENERATED_HEADER}{BUSY_GO}"));
    project.file("README.md", "# notes\n");
    let files = ["src/vite-env.d.ts", "gen/api.pb.go", "README.md"];

    let output = hook(&project, &files);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
    assert_eq!(stderr(&output), "");

    let mut without_flag = vec!["--config", "."];
    without_flag.extend_from_slice(&files);
    let output = project.run(&without_flag);
    assert_eq!(code(&output), 1);
    assert!(
        stderr(&output).contains("no supported files found"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn allow_no_files_still_fails_on_a_missing_path() {
    let project = Project::new();
    project.file("src/a.php", BUSY_PHP);

    let output = hook(&project, &["missing.php"]);

    assert_eq!(code(&output), 1);
    assert!(
        stderr(&output).contains("could not be read"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn an_explicitly_named_file_matching_exclude_is_dropped() {
    let project = Project::new();
    project.file(
        "bonsai-lint.toml",
        "threshold = 1\nexclude = [\"**/*.spec.ts\"]\n",
    );
    project.file("src/a.spec.ts", BUSY_TS);
    project.file("src/b.ts", BUSY_TS);

    let output = hook(&project, &["src/a.spec.ts", "src/b.ts"]);

    assert_eq!(code(&output), 1);
    assert!(stdout(&output).contains("src/b.ts"), "{}", stdout(&output));
    assert!(
        !stdout(&output).contains("a.spec.ts"),
        "{}",
        stdout(&output)
    );

    let output = hook(&project, &["src/a.spec.ts"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
}

/// Without `--config .`, discovery starts at the first file, so a batch opening with a file under
/// an undeclared config would be judged by that config and the next batch would not.
#[test]
fn every_batch_is_judged_by_the_root_config_whatever_file_comes_first() {
    let project = Project::new();
    project.file("sub/bonsai-lint.toml", "threshold = 0\n");
    project.file("sub/busy.php", BUSY_PHP);
    project.file("top/busy.ts", BUSY_TS);
    let whole_repo = project.run(&[]);

    let php_first = hook(&project, &["sub/busy.php", "top/busy.ts"]);
    let ts_first = hook(&project, &["top/busy.ts", "sub/busy.php"]);

    assert_eq!(code(&php_first), code(&whole_repo));
    assert_eq!(code(&ts_first), code(&whole_repo));
    assert_eq!(stdout(&php_first), stdout(&whole_repo));
    assert_eq!(stdout(&ts_first), stdout(&whole_repo));
}
