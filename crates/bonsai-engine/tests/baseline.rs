//! A baseline is an accepted debt ledger: it must round-trip exactly, refuse formats it does not
//! understand, never turn an unknown unit into an acceptance, and name every entry the code has
//! outgrown.

use std::fs;
use std::path::PathBuf;

use bonsai_core::{Finding, NameOrigin, Suppression};
use bonsai_engine::{Baseline, Located, Looseness};

const THRESHOLD: u32 = 15;

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

fn marked(key_path: &str, name: &str, score: u32, suppression: Suppression) -> Located {
    let mut item = located(key_path, name, score);
    item.finding.suppression = suppression;
    item
}

fn loose(baseline: &Baseline, scanned: &[Located]) -> Vec<(String, String, u32, Looseness)> {
    baseline
        .loose(scanned, |_| THRESHOLD)
        .into_iter()
        .map(|entry| (entry.key_path, entry.unit, entry.recorded, entry.reason))
        .collect()
}

fn entry(
    key_path: &str,
    unit: &str,
    recorded: u32,
    reason: Looseness,
) -> (String, String, u32, Looseness) {
    (key_path.to_string(), unit.to_string(), recorded, reason)
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
fn an_entry_matching_nothing_is_loose() {
    let baseline = Baseline::from_findings(&[
        located("src/a.php", "busy", 20),
        located("src/b.php", "gone", 20),
    ]);

    assert_eq!(
        loose(&baseline, &[located("src/a.php", "busy", 3)]),
        [
            entry(
                "src/a.php",
                "busy",
                20,
                Looseness::AtOrUnderThreshold {
                    score: 3,
                    threshold: THRESHOLD
                }
            ),
            entry("src/b.php", "gone", 20, Looseness::Unmatched),
        ]
    );
}

/// A unit that has come down to its threshold needs no entry, and keeping one would let it grow
/// back to the old score unnoticed.
#[test]
fn a_unit_at_its_threshold_is_loose() {
    let baseline = Baseline::from_findings(&[located("src/a.php", "busy", 21)]);

    assert_eq!(
        loose(&baseline, &[located("src/a.php", "busy", THRESHOLD)]),
        [entry(
            "src/a.php",
            "busy",
            21,
            Looseness::AtOrUnderThreshold {
                score: THRESHOLD,
                threshold: THRESHOLD
            }
        )]
    );
}

#[test]
fn an_improved_unit_still_over_is_loose() {
    let baseline = Baseline::from_findings(&[located("src/a.php", "busy", 38)]);

    assert_eq!(
        loose(&baseline, &[located("src/a.php", "busy", 20)]),
        [entry(
            "src/a.php",
            "busy",
            38,
            Looseness::Lower { score: 20 }
        )]
    );
}

/// An unchanged unit is what a baseline is for, and a worse one is already a breach.
#[test]
fn an_unchanged_or_worse_unit_is_not_loose() {
    let baseline = Baseline::from_findings(&[located("src/a.php", "busy", 20)]);

    assert!(loose(&baseline, &[located("src/a.php", "busy", 20)]).is_empty());
    assert!(loose(&baseline, &[located("src/a.php", "busy", 25)]).is_empty());
}

/// A reasoned marker keeps the unit out of any baseline written from now on, so its entry is dead
/// weight.
#[test]
fn a_reasoned_suppression_leaves_the_entry_loose() {
    let baseline = Baseline::from_findings(&[located("src/a.php", "busy", 20)]);
    let suppressed = marked(
        "src/a.php",
        "busy",
        20,
        Suppression::Reasoned("legacy".to_string()),
    );

    assert_eq!(
        loose(&baseline, &[suppressed]),
        [entry("src/a.php", "busy", 20, Looseness::Suppressed)]
    );
}

#[test]
fn a_marker_without_a_reason_does_not() {
    let baseline = Baseline::from_findings(&[located("src/a.php", "busy", 20)]);
    let unreasoned = marked("src/a.php", "busy", 20, Suppression::MissingReason);

    assert!(loose(&baseline, &[unreasoned]).is_empty());
}

/// Two accessors the namer cannot tell apart share one entry holding the higher score, so the
/// lower one must not read as an improvement.
#[test]
fn twins_are_judged_by_the_higher_score() {
    let twins = [
        located("src/a.php", "set", 20),
        located("src/a.php", "set", 0),
    ];
    let baseline = Baseline::from_findings(&twins[..1]);

    assert!(loose(&baseline, &twins).is_empty());
}

#[test]
fn a_suppressed_twin_is_ignored() {
    let baseline = Baseline::from_findings(&[located("src/a.php", "busy", 38)]);
    let suppressed = marked(
        "src/a.php",
        "busy",
        40,
        Suppression::Reasoned("legacy".to_string()),
    );

    assert_eq!(
        loose(
            &baseline,
            &[suppressed.clone(), located("src/a.php", "busy", 20)]
        ),
        [entry(
            "src/a.php",
            "busy",
            38,
            Looseness::Lower { score: 20 }
        )]
    );
    assert_eq!(
        loose(&baseline, &[suppressed, located("src/a.php", "busy", 3)]),
        [entry(
            "src/a.php",
            "busy",
            38,
            Looseness::AtOrUnderThreshold {
                score: 3,
                threshold: THRESHOLD
            }
        )]
    );
}

#[test]
fn the_threshold_is_each_units_own() {
    let mut go = located("src/a.go", "busy", 20);
    go.finding.language = "go";
    let scanned = [located("src/a.php", "busy", 20), go];
    let baseline = Baseline::from_findings(&scanned);

    let found = baseline.loose(&scanned, |item| match item.finding.language {
        "go" => 25,
        _ => THRESHOLD,
    });

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].key_path, "src/a.go");
    assert_eq!(
        found[0].reason,
        Looseness::AtOrUnderThreshold {
            score: 20,
            threshold: 25
        }
    );
}

/// The definition the rest rests on: an entry is loose exactly when rewriting the baseline from
/// the same scan would drop or lower it, so a baseline written from a scan is never loose against
/// that scan.
#[test]
fn a_freshly_written_baseline_is_never_loose() {
    let scanned = [
        located("src/a.php", "busy", 30),
        located("src/a.php", "calm", 3),
        located("src/a.php", "edge", THRESHOLD),
        located("src/b.php", "set", 22),
        located("src/b.php", "set", 1),
        marked(
            "src/b.php",
            "hushed",
            40,
            Suppression::Reasoned("legacy".to_string()),
        ),
        marked("src/c.php", "loud", 25, Suppression::MissingReason),
    ];
    let recorded: Vec<Located> = scanned
        .iter()
        .filter(|item| item.finding.score > THRESHOLD && !item.finding.is_suppressed())
        .cloned()
        .collect();
    let baseline = Baseline::from_findings(&recorded);

    assert_eq!(baseline.len(), 3);
    assert!(loose(&baseline, &scanned).is_empty());
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
