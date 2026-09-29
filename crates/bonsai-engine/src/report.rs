//! The JSON report that `--format json` prints and editors read.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::baseline::{LooseEntry, Looseness};

/// The document `bonsai-lint --format json` prints.
#[derive(Debug, Serialize)]
pub struct Report {
    /// The threshold per language id, from `--domain` or the one domain the scan visited, else the
    /// root's.
    pub thresholds: BTreeMap<String, u32>,
    /// How many findings fail the run; under `--all`, `findings` also lists those that pass.
    pub breaches: usize,
    /// The reported units, worst first.
    pub findings: Vec<ReportedFinding>,
    /// Baseline entries looser than the code, from every baseline this scan could judge.
    ///
    /// Only a scan covering a baseline's whole domain judges it, so one naming a single file, or
    /// run with `--stdin`, `--domain` or `--lang`, leaves this empty.
    pub loose_entries: Vec<ReportedLooseEntry>,
}

/// One unit in a [`Report`].
#[derive(Debug, Serialize)]
pub struct ReportedFinding {
    /// The file's path, relative when the scanned path was, with forward slashes on every platform.
    pub path: String,
    /// The 1-based line the unit starts on.
    pub line: usize,
    /// The unit's qualified name, such as `Class::method`, which its baseline entry is keyed by.
    pub name: String,
    /// The unit's cognitive complexity.
    pub score: u32,
    /// The language id, as `--lang` and `--over` take it.
    pub language: &'static str,
    /// The name of the domain the file resolved to.
    pub domain: String,
}

/// A baseline entry that rewriting the baseline over the same scan would drop or lower.
#[derive(Debug, Serialize)]
pub struct ReportedLooseEntry {
    /// The domain whose baseline holds the entry, `None` for the one file `--baseline` names.
    pub domain: Option<String>,
    /// The file the entry is keyed under, relative to that baseline's root.
    pub path: String,
    /// The unit's qualified name, as the baseline records it.
    pub unit: String,
    /// The score the baseline accepts.
    pub recorded: u32,
    /// Why the entry is looser than the code.
    pub reason: LooseReason,
    /// The unit's score now, `None` when it matched nothing or is suppressed.
    pub score: Option<u32>,
    /// The threshold the unit is held to, given only when it no longer scores over it.
    pub threshold: Option<u32>,
    /// Whether the entry's baseline is strict, which makes the entry fail the run.
    pub strict: bool,
}

impl ReportedLooseEntry {
    /// Builds the report's form of `entry`, found in the baseline of `domain`.
    #[must_use]
    pub fn new(domain: Option<String>, entry: &LooseEntry, strict: bool) -> Self {
        let (reason, score, threshold) = match entry.reason {
            Looseness::Unmatched => (LooseReason::Unmatched, None, None),
            Looseness::Suppressed => (LooseReason::Suppressed, None, None),
            Looseness::NotOverThreshold { score, threshold } => {
                (LooseReason::NotOverThreshold, Some(score), Some(threshold))
            }
            Looseness::Lower { score } => (LooseReason::Lower, Some(score), None),
        };
        Self {
            domain,
            path: entry.key_path.clone(),
            unit: entry.unit.clone(),
            recorded: entry.recorded,
            reason,
            score,
            threshold,
            strict,
        }
    }
}

/// Why a baseline entry is looser than the code, as the JSON report spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LooseReason {
    /// No unit in the scan has the entry's key, as after a deletion or a rename.
    Unmatched,
    /// Every unit with the entry's key carries a suppression marker with a reason.
    Suppressed,
    /// The unit no longer scores over its threshold, so it needs no entry.
    NotOverThreshold,
    /// The unit is still over its threshold, but scores less than the entry accepts.
    Lower,
}
