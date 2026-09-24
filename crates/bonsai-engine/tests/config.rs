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

fn error_of(root: &Path) -> String {
    config::discover(root, KNOWN)
        .expect_err("discovery refuses the config")
        .to_string()
}

#[test]
fn a_mistyped_top_level_key_is_named_with_a_suggestion() {
    let (_dir, root) = project();
    write(&root, "bonsai-lint.toml", "treshold = 10\n");

    let error = error_of(&root);

    assert!(error.contains("unknown key `treshold`"), "{error}");
    assert!(error.contains("did you mean `threshold`?"), "{error}");
}

#[test]
fn an_unknown_top_level_key_is_named_without_a_guess() {
    let (_dir, root) = project();
    write(&root, "bonsai-lint.toml", "colour = \"green\"\n");

    let error = error_of(&root);

    assert!(error.contains("unknown key `colour`"), "{error}");
    assert!(!error.contains("did you mean"), "{error}");
}

#[test]
fn a_misspelt_language_section_suggests_the_language() {
    let (_dir, root) = project();
    write(&root, "bonsai-lint.toml", "[typscript]\nthreshold = 3\n");

    let workspace = discover(&root);

    assert!(
        workspace.warnings.iter().any(
            |warning| warning.contains("`[typscript]`, ignoring; did you mean `[typescript]`?")
        ),
        "{:?}",
        workspace.warnings
    );
}

/// `--domain` and the report's `domain` field could not tell the two apart.
#[test]
fn two_domains_with_one_name_are_rejected() {
    let (_dir, root) = project();
    write(&root, "bonsai-lint.toml", "domains = [\"apps/*\"]\n");
    write(&root, "apps/web/bonsai-lint.toml", "name = \"shop\"\n");
    write(&root, "apps/admin/bonsai-lint.toml", "name = \"shop\"\n");

    let error = error_of(&root);

    assert!(error.contains("apps/web/bonsai-lint.toml"), "{error}");
    assert!(
        error.contains("`shop` is also the name of `apps/admin`"),
        "{error}"
    );
}

#[test]
fn a_domain_named_root_collides_with_the_workspace_root() {
    let (_dir, root) = project();
    write(&root, "bonsai-lint.toml", "domains = [\"apps/*\"]\n");
    write(&root, "apps/web/bonsai-lint.toml", "name = \"root\"\n");

    let error = error_of(&root);

    assert!(
        error.contains("`root` is also the name of the workspace root"),
        "{error}"
    );
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
    let warnings = workspace.undeclared_config_warnings(std::slice::from_ref(&root));
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("not a declared domain"),
        "{warnings:?}"
    );
}

/// A stray config elsewhere in the repository never touched the files being scanned; one above
/// or below them might have.
#[test]
fn undeclared_configs_are_reported_only_where_the_scan_looked() {
    let (_dir, root) = project();
    write(&root, "bonsai-lint.toml", "threshold = 10\n");
    write(&root, "packages/stray/bonsai-lint.toml", "threshold = 1\n");
    write(&root, "packages/stray/src/a.php", "<?php\n");
    write(&root, "packages/clean/src/a.php", "<?php\n");

    let workspace = discover(&root);

    let elsewhere = workspace.undeclared_config_warnings(&[root.join("packages/clean")]);
    assert!(elsewhere.is_empty(), "{elsewhere:?}");

    let above = workspace.undeclared_config_warnings(&[root.join("packages/stray/src")]);
    assert_eq!(above.len(), 1, "{above:?}");

    let both =
        workspace.undeclared_config_warnings(&[root.join("packages/stray/src"), root.clone()]);
    assert_eq!(both.len(), 1, "{both:?}");
}

/// The root is the config that declares `domains`. Starting the scan inside a domain must not
/// promote the domain's own config to root, or the root's excludes, inherited thresholds and
/// sibling domains would silently vanish.
#[test]
fn a_scan_started_inside_a_domain_keeps_the_root() {
    let (_dir, root) = project();
    write(
        &root,
        "bonsai-lint.toml",
        "domains = [\"apps/*\", \"packages/*\"]\nthreshold = 20\nexclude = [\"**/*.spec.ts\"]\n",
    );
    write(
        &root,
        "apps/wallet/bonsai-lint.toml",
        "name = \"wallet\"\nthreshold = 8\n",
    );
    write(&root, "packages/calc/bonsai-lint.toml", "name = \"calc\"\n");

    for start in ["apps/wallet", "apps/wallet/src", "packages/calc"] {
        let workspace = discover(&root.join(start));
        assert_eq!(workspace.root, root, "started at {start}");
        assert_eq!(domain_names(&workspace), vec!["root", "wallet", "calc"]);
        assert!(workspace.is_excluded(&root.join("apps/wallet/a.spec.ts")));
    }

    let workspace = discover(&root.join("packages/calc"));
    let calc = &workspace.domains[workspace.domain_for(&root.join("packages/calc/a.ts"))];
    assert_eq!(
        calc.threshold, 20,
        "inherits the root, not the built-in default"
    );
    let wallet = &workspace.domains[workspace.domain_for(&root.join("apps/wallet/a.ts"))];
    assert_eq!(wallet.threshold, 8);
}

#[test]
fn a_domain_declaring_domains_of_its_own_does_not_become_the_root() {
    let (_dir, root) = project();
    write(&root, "bonsai-lint.toml", "domains = [\"apps/*\"]\n");
    write(
        &root,
        "apps/wallet/bonsai-lint.toml",
        "domains = [\"packages/*\"]\n",
    );
    write(&root, "apps/wallet/packages/x/a.ts", "");

    let workspace = discover(&root.join("apps/wallet/packages/x"));

    assert_eq!(workspace.root, root);
    assert_eq!(domain_names(&workspace), vec!["root", "apps/wallet"]);
    assert!(
        workspace
            .warnings
            .iter()
            .any(|w| w.contains("only read from the root config")),
        "{:?}",
        workspace.warnings
    );
}

/// Without a `domains` declaration nothing claims the nested config, so the nearest one is the
/// project, exactly as a root scan's undeclared-config warning says it is being ignored there.
#[test]
fn without_a_declaring_config_the_nearest_one_is_the_root() {
    let (_dir, root) = project();
    write(&root, "bonsai-lint.toml", "threshold = 10\n");
    write(&root, "packages/own/bonsai-lint.toml", "threshold = 1\n");

    let workspace = discover(&root.join("packages/own"));

    assert_eq!(workspace.root, root.join("packages/own"));
    assert_eq!(workspace.domains[0].threshold, 1);
}
