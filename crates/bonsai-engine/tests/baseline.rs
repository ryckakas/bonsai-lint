//! A baseline is an accepted debt ledger: it must round-trip exactly, refuse formats it does not
//! understand, and never turn an unknown unit into an acceptance.

use std::fs;
use std::path::PathBuf;

use bonsai_core::{Finding, NameOrigin, Suppression};
use bonsai_engine::{Baseline, Located};

fn located(key_path: &str, name: &str, score: u32) -> Located {
    Located {
        path: PathBuf::from(key_path),
        key_path: key_path.to_string(),
        domain: 0,
        finding: Finding {
            container: None,
            name: name.to_string(),
            origin: NameOrigin::Declared,
            line: 1,
            score,
            suppression: Suppression::None,
            language: "php",
        },
    }
}

#[test]
fn a_baseline_survives_a_round_trip_through_disk() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("nested/.bonsai-lint-baseline.json");
    let recorded = [
        located("src/a.php", "busy", 20),
        located("src/b.php", "busier", 30),
    ];

    Baseline::from_findings(&recorded)
        .save(&path)
        .expect("save creates parent directories");
    let loaded = Baseline::load(&path).expect("load");

    assert_eq!(loaded.len(), 2);
    assert!(recorded.iter().all(|item| !loaded.is_regression(item)));
}

#[test]
fn a_higher_score_is_a_regression_and_an_equal_one_is_accepted() {
    let baseline = Baseline::from_findings(&[located("src/a.php", "busy", 20)]);

    assert!(!baseline.is_regression(&located("src/a.php", "busy", 20)));
    assert!(!baseline.is_regression(&located("src/a.php", "busy", 19)));
    assert!(baseline.is_regression(&located("src/a.php", "busy", 21)));
}

/// A renamed unit must not inherit another unit's amnesty.
#[test]
fn an_unknown_unit_is_a_regression() {
    let baseline = Baseline::from_findings(&[located("src/a.php", "busy", 20)]);

    assert!(baseline.is_regression(&located("src/a.php", "renamed", 1)));
    assert!(baseline.is_regression(&located("src/moved.php", "busy", 1)));
}

/// Two units the namer cannot tell apart share one entry, and an unchanged rerun must accept
/// both, whichever order they arrive in.
#[test]
fn units_sharing_a_key_keep_the_higher_score() {
    let higher = located("src/a.php", "busy", 30);
    let lower = located("src/a.php", "busy", 20);
    for recorded in [[higher.clone(), lower.clone()], [lower, higher]] {
        let baseline = Baseline::from_findings(&recorded);

        assert_eq!(baseline.len(), 1);
        assert!(recorded.iter().all(|item| !baseline.is_regression(item)));
        assert!(baseline.is_regression(&located("src/a.php", "busy", 31)));
    }
}

#[test]
fn entries_that_match_nothing_are_reported() {
    let baseline = Baseline::from_findings(&[
        located("src/a.php", "busy", 20),
        located("src/b.php", "gone", 20),
    ]);

    let stale = baseline.unmatched(&[located("src/a.php", "busy", 3)]);

    assert_eq!(stale, vec!["src/b.php: gone".to_string()]);
}

#[test]
fn an_unsupported_format_version_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(".bonsai-lint-baseline.json");
    fs::write(&path, "{\"version\": 2, \"entries\": {}}").expect("write");

    let error = Baseline::load(&path).expect_err("version 2 is unknown");

    assert!(error.to_string().contains("regenerate"), "{error}");
}

#[test]
fn a_corrupt_baseline_is_an_error_not_an_empty_ledger() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(".bonsai-lint-baseline.json");
    fs::write(&path, "{ not json").expect("write");

    assert!(Baseline::load(&path).is_err());
}
