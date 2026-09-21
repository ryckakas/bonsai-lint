//! Per-domain configuration, thresholds and baselines across a multi-domain workspace.

use std::fs;

mod common;

use common::{
    assert_matches_serial, baselines, code, nested_php, nested_ts, paths, remove_baselines, report,
    stderr, stdout, Project, BUSY_TS, CALM_PHP, CALM_TS,
};
/// A monorepo's shape: several domains at once, each with its own config, threshold and
/// baseline. The engine tests cover two domains; the combination below is what a real repository
/// runs and what only ever got checked by hand.
const DOMAINS: &[&str] = &[
    "apps/lenient",
    "apps/strict",
    "apps/flat",
    "apps/plain",
    "packages/scoped",
    "packages/plain",
];

/// `heavy` scores 21 and `mid` scores 6, so they straddle the per-domain thresholds below.
fn seed_domain(project: &Project, domain: &str) {
    for index in 0..3 {
        project.file(
            &format!("{domain}/heavy{index}.php"),
            &nested_php(&format!("heavy{index}"), 6),
        );
        project.file(
            &format!("{domain}/mid{index}.ts"),
            &nested_ts(&format!("mid{index}"), 3),
        );
        project.file(&format!("{domain}/calm{index}.php"), CALM_PHP);
    }
}

fn multi_domain_project() -> Project {
    let project = Project::new();
    project.file(
        "bonsai-lint.toml",
        "domains = [\"apps/*\", \"packages/*\"]\nthreshold = 12\nexclude = [\"**/vendor/**\"]\n\n[php]\nthreshold = 20\n\n[typescript]\nthreshold = 8\n",
    );
    project.file("apps/lenient/bonsai-lint.toml", "threshold = 30\n");
    project.file(
        "apps/strict/bonsai-lint.toml",
        "threshold = 5\n\n[typescript]\nthreshold = 4\n",
    );
    project.file("apps/flat/bonsai-lint.toml", "toplevel = false\n");
    project.file(
        "packages/scoped/bonsai-lint.toml",
        "exclude = [\"generated/**\"]\n",
    );

    for domain in DOMAINS {
        seed_domain(&project, domain);
    }

    // Several warnings, spread across the walk: one alone cannot be emitted out of order.
    for (index, domain) in DOMAINS.iter().enumerate().take(4) {
        let mut legacy = nested_php(&format!("legacy{index}"), 6).into_bytes();
        legacy.extend_from_slice(b"// caf\xE9\n");
        fs::write(project.root.join(format!("{domain}/legacy.php")), legacy).expect("write");
    }

    project.file("apps/plain/vendor/bundled.php", &nested_php("bundled", 6));
    project.file(
        "packages/scoped/generated/out.php",
        &nested_php("generated", 6),
    );
    project.file(
        "apps/flat/script.php",
        "<?php\nif ($a) { if ($a) { echo 1; } }\n",
    );
    project.file(
        "apps/plain/script.php",
        "<?php\nif ($a) { if ($a) { echo 1; } }\n",
    );
    project
}

fn inject_ghost(project: &Project, domain: &str) {
    let relative = format!("{domain}/.bonsai-lint-baseline.json");
    let mut baseline: serde_json::Value =
        serde_json::from_str(&project.read(&relative)).expect("baseline is json");
    baseline["entries"]["ghost.php"] = serde_json::json!({ "ghost": 99 });
    fs::write(
        project.root.join(&relative),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&baseline).expect("serialize")
        ),
    )
    .expect("write baseline");
}

/// A per-app CI job, a pre-commit hook and a developer inside one package all point the CLI at
/// a domain directory. The answer must be the one the root scan gives.
#[test]
fn a_scan_started_inside_a_domain_agrees_with_one_from_the_root() {
    let project = Project::new();
    project
        .file(
            "bonsai-lint.toml",
            "domains = [\"apps/*\"]\nthreshold = 15\nexclude = [\"**/*.spec.ts\"]\n",
        )
        .file(
            "apps/wallet/bonsai-lint.toml",
            "name = \"wallet\"\nthreshold = 2\n",
        )
        .file("apps/wallet/src/a.ts", BUSY_TS)
        .file("apps/wallet/src/a.spec.ts", BUSY_TS)
        .file("src/calm.ts", CALM_TS);

    let from_root = project.run(&["--format", "json", "."]);
    let from_domain = project.run(&["--format", "json", "apps/wallet"]);
    let from_file = project.run(&["--format", "json", "apps/wallet/src/a.ts"]);

    for output in [&from_root, &from_domain, &from_file] {
        assert_eq!(code(output), 1, "{}", stderr(output));
        assert_eq!(report(output)["breaches"], 1);
        assert_eq!(
            paths(output),
            vec!["apps/wallet/src/a.ts"],
            "spec files stay excluded"
        );
    }

    // The thresholds a report claims are the ones that gated it: the root's across domains,
    // the domain's own when the scan stayed inside one.
    assert_eq!(report(&from_root)["thresholds"]["typescript"], 15);
    assert_eq!(report(&from_domain)["thresholds"]["typescript"], 2);
    assert_eq!(report(&from_file)["thresholds"]["typescript"], 2);
}

#[test]
fn a_stray_config_is_reported_only_when_the_scan_passed_through_it() {
    let project = Project::new();
    project
        .file(
            "bonsai-lint.toml",
            "domains = [\"apps/*\"]\nthreshold = 15\n",
        )
        .file("packages/stray/bonsai-lint.toml", "threshold = 1\n")
        .file("packages/stray/src/a.php", CALM_PHP)
        .file("packages/clean/src/a.php", CALM_PHP);

    let through = project.run(&["packages/stray/src"]);
    let elsewhere = project.run(&["packages/clean"]);

    assert!(
        stderr(&through).contains("not a declared domain"),
        "{}",
        stderr(&through)
    );
    assert!(stderr(&elsewhere).is_empty(), "{}", stderr(&elsewhere));
}

#[test]
fn many_domains_scan_identically_however_many_workers_run() {
    let project = multi_domain_project();

    let reference = project.run(&["--all", "--format", "json", "--jobs", "1", "."]);
    assert!(
        paths(&reference).len() > 50,
        "the comparison is only meaningful if the scan reports plenty"
    );

    for format in ["text", "json"] {
        let serial = project.run(&["--all", "--format", format, "--jobs", "1", "."]);
        for jobs in ["2", "4", "8", "0"] {
            assert_matches_serial(&project, format, jobs, &serial);
        }
    }
}

#[test]
fn each_domain_applies_its_own_threshold() {
    let project = multi_domain_project();

    let breached = paths(&project.run(&["--format", "json", "."]));

    assert!(
        !breached
            .iter()
            .any(|path| path.starts_with("apps/lenient/")),
        "a threshold of 30 accepts a score of 21: {breached:?}"
    );
    assert!(breached.contains(&"apps/strict/heavy0.php".to_string()));
    assert!(breached.contains(&"apps/plain/heavy0.php".to_string()));

    assert!(
        breached.contains(&"apps/strict/mid0.ts".to_string()),
        "6 is over the domain's typescript threshold of 4"
    );
    assert!(
        !breached.contains(&"apps/plain/mid0.ts".to_string()),
        "6 is under the root's typescript threshold of 8"
    );

    assert!(!breached.iter().any(|path| path.contains("/vendor/")));
    assert!(!breached.iter().any(|path| path.contains("/generated/")));
}

#[test]
fn a_domain_can_turn_off_toplevel_scoring_without_affecting_its_neighbours() {
    let project = multi_domain_project();

    let reported = paths(&project.run(&["--all", "--format", "json", "."]));

    assert!(!reported.contains(&"apps/flat/script.php".to_string()));
    assert!(reported.contains(&"apps/plain/script.php".to_string()));
}

#[test]
fn a_baseline_per_domain_is_written_identically_however_many_workers_run() {
    let project = multi_domain_project();

    let written = project.run(&["--write-baseline", "--jobs", "1", "."]);
    assert_eq!(code(&written), 0, "{}", stderr(&written));

    let expected = baselines(&project);
    let names: Vec<&String> = expected.iter().map(|(path, _)| path).collect();
    assert!(
        expected.len() >= 4,
        "several domains must record their own baseline: {names:?}"
    );
    assert!(names.iter().any(|path| path.starts_with("apps/strict/")));
    assert!(names.iter().any(|path| path.starts_with("packages/plain/")));

    for jobs in ["2", "8", "16"] {
        remove_baselines(&project);
        let parallel = project.run(&["--write-baseline", "--jobs", jobs, "."]);
        assert_eq!(code(&parallel), 0, "{}", stderr(&parallel));
        assert_eq!(baselines(&project), expected, "jobs={jobs}");
    }
}

#[test]
fn every_domain_baseline_is_honoured_at_once() {
    let project = multi_domain_project();
    assert_eq!(code(&project.run(&["--write-baseline", "."])), 0);

    let accepted = project.run(&["--jobs", "8", "."]);

    assert_eq!(code(&accepted), 0, "{}", stderr(&accepted));
    assert_eq!(stdout(&accepted), "");
}

#[test]
fn stale_entries_are_reported_for_each_domain_that_has_them() {
    let project = multi_domain_project();
    assert_eq!(code(&project.run(&["--write-baseline", "."])), 0);
    inject_ghost(&project, "apps/strict");
    inject_ghost(&project, "packages/plain");

    let output = project.run(&["--jobs", "8", "."]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("apps/strict: 1 baseline entr(ies) matched nothing"),
        "{}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("packages/plain: 1 baseline entr(ies) matched nothing"),
        "{}",
        stderr(&output)
    );
}
