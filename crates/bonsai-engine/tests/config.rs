//! Configuration discovery is what turns a directory tree into domains, thresholds and
//! excludes; every rule here is one a user can observe from `bonsai-lint.toml` alone.

use std::fs;
use std::path::{Path, PathBuf};

use bonsai_engine::config::{self, ConfigError, Workspace};

const KNOWN: &[&str] = &["php", "typescript"];

/// Canonical, as the CLI hands it over, so `/tmp` and `/private/tmp` cannot disagree on macOS.
fn project() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().canonicalize().expect("canonical temp dir");
    (dir, root)
}

fn write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("has parent")).expect("mkdir");
    fs::write(path, content).expect("write");
}

fn discover(root: &Path) -> Workspace {
    config::discover(root, KNOWN).expect("discovery succeeds")
}

fn domain_names(workspace: &Workspace) -> Vec<&str> {
    workspace
        .domains
        .iter()
        .map(|domain| domain.name.as_str())
        .collect()
}

#[test]
fn domain_globs_do_not_cross_directory_separators() {
    let (_dir, root) = project();
    write(&root, "bonsai-lint.toml", "domains = [\"packages/*\"]\n");
    write(&root, "packages/web/src/deep/index.ts", "");
    write(&root, "packages/api/src/index.ts", "");

    let workspace = discover(&root);

    assert_eq!(
        domain_names(&workspace),
        vec!["root", "packages/api", "packages/web"]
    );
    let web = workspace.domain_for(&root.join("packages/web/src/deep/index.ts"));
    assert_eq!(workspace.domains[web].name, "packages/web");
    assert_eq!(
        workspace.domains[web].baseline,
        root.join("packages/web/.bonsai-lint-baseline.json")
    );
}

#[test]
fn a_double_star_still_names_a_whole_subtree() {
    let (_dir, root) = project();
    write(
        &root,
        "bonsai-lint.toml",
        "domains = [\"services/**/app\"]\n",
    );
    write(&root, "services/billing/app/main.php", "");

    assert_eq!(
        domain_names(&discover(&root)),
        vec!["root", "services/billing/app"]
    );
}

#[test]
fn exclude_globs_follow_the_same_separator_rule() {
    let (_dir, root) = project();
    write(
        &root,
        "bonsai-lint.toml",
        "exclude = [\"*.generated.ts\", \"vendor/**\"]\n",
    );

    let workspace = discover(&root);

    assert!(workspace.is_excluded(&root.join("a.generated.ts")));
    assert!(!workspace.is_excluded(&root.join("src/a.generated.ts")));
    assert!(workspace.is_excluded(&root.join("vendor/lib/deep/x.php")));
}

#[test]
fn a_file_as_the_starting_point_uses_its_directory() {
    let (_dir, root) = project();
    write(&root, "src/Foo.php", "<?php\n");

    let workspace = discover(&root.join("src/Foo.php"));

    assert_eq!(workspace.root, root.join("src"));
    assert_eq!(
        workspace.domains[0].baseline,
        root.join("src/.bonsai-lint-baseline.json")
    );
}

#[test]
fn without_a_config_an_existing_baseline_marks_the_root() {
    let (_dir, root) = project();
    write(
        &root,
        ".bonsai-lint-baseline.json",
        "{\"version\":1,\"entries\":{}}",
    );
    write(&root, "src/Foo.php", "<?php\n");

    let workspace = discover(&root.join("src/Foo.php"));

    assert_eq!(workspace.root, root);
}

#[test]
fn a_mistyped_language_key_is_rejected() {
    let (_dir, root) = project();
    write(&root, "bonsai-lint.toml", "[typescript]\nthresold = 20\n");

    assert!(matches!(
        config::discover(&root, KNOWN),
        Err(ConfigError::Parse { .. })
    ));
}

#[test]
fn a_mistyped_top_level_key_is_rejected() {
    let (_dir, root) = project();
    write(&root, "bonsai-lint.toml", "thresold = 20\n");

    assert!(matches!(
        config::discover(&root, KNOWN),
        Err(ConfigError::Parse { .. })
    ));
}

#[test]
fn a_negated_exclude_is_refused_rather_than_ignored() {
    let (_dir, root) = project();
    write(&root, "bonsai-lint.toml", "exclude = [\"!vendor/keep\"]\n");

    match config::discover(&root, KNOWN) {
        Err(ConfigError::Glob { pattern, error }) => {
            assert_eq!(pattern, "!vendor/keep");
            assert!(error.contains("negated"), "{error}");
        }
        other => panic!("expected a glob error, got {other:?}"),
    }
}

#[test]
fn a_domain_excludes_relative_to_its_own_root() {
    let (_dir, root) = project();
    write(
        &root,
        "bonsai-lint.toml",
        "domains = [\"packages/*\"]\nexclude = [\"vendor/**\"]\n",
    );
    write(
        &root,
        "packages/web/bonsai-lint.toml",
        "exclude = [\"generated/**\"]\n",
    );
    write(&root, "packages/api/.keep", "");

    let workspace = discover(&root);

    assert!(workspace.is_excluded(&root.join("packages/web/generated/a.ts")));
    assert!(!workspace.is_excluded(&root.join("packages/api/generated/a.ts")));
    assert!(workspace.is_excluded(&root.join("vendor/x.php")));
    assert!(!workspace.is_excluded(&root.join("packages/web/src/a.ts")));
}

#[test]
fn domains_declared_outside_the_root_are_ignored_with_a_warning() {
    let (_dir, root) = project();
    write(&root, "bonsai-lint.toml", "domains = [\"packages/*\"]\n");
    write(
        &root,
        "packages/web/bonsai-lint.toml",
        "domains = [\"apps/*\"]\n",
    );

    let workspace = discover(&root);

    assert_eq!(domain_names(&workspace), vec!["root", "packages/web"]);
    assert!(
        workspace
            .warnings
            .iter()
            .any(|warning| warning.contains("only read from the root config")),
        "{:?}",
        workspace.warnings
    );
}

#[test]
fn the_nearest_and_most_specific_threshold_wins() {
    let (_dir, root) = project();
    write(
        &root,
        "bonsai-lint.toml",
        "domains = [\"packages/*\"]\nthreshold = 10\n\n[typescript]\nthreshold = 20\n",
    );
    write(&root, "packages/web/bonsai-lint.toml", "threshold = 5\n");
    write(&root, "packages/api/.keep", "");

    let workspace = discover(&root);
    let by_name = |name: &str| {
        workspace
            .domains
            .iter()
            .find(|domain| domain.name == name)
            .expect("domain exists")
    };

    assert_eq!(by_name("root").threshold_for("typescript"), 20);
    assert_eq!(by_name("root").threshold_for("php"), 10);
    assert_eq!(by_name("packages/web").threshold_for("typescript"), 5);
    assert_eq!(by_name("packages/web").threshold_for("php"), 5);
    assert_eq!(by_name("packages/api").threshold_for("typescript"), 20);
    assert_eq!(by_name("packages/api").threshold_for("php"), 10);
}

#[test]
fn an_unknown_language_section_warns() {
    let (_dir, root) = project();
    write(&root, "bonsai-lint.toml", "[ruby]\nthreshold = 3\n");

    let workspace = discover(&root);

    assert!(
        workspace
            .warnings
            .iter()
            .any(|warning| warning.contains("unknown language section `[ruby]`")),
        "{:?}",
        workspace.warnings
    );
}

/// The tree walk behind this warning is the one cost a per-keystroke scan cannot afford, so it
/// is not part of discovery itself.
#[test]
fn an_undeclared_config_is_reported_only_on_request() {
    let (_dir, root) = project();
    write(&root, "bonsai-lint.toml", "threshold = 10\n");
    write(&root, "packages/stray/bonsai-lint.toml", "threshold = 1\n");

    let workspace = discover(&root);

    assert!(workspace.warnings.is_empty(), "{:?}", workspace.warnings);
    let warnings = workspace.undeclared_config_warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("not a declared domain"),
        "{warnings:?}"
    );
}
