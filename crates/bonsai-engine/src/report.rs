//! The JSON report that `--format json` prints and editors read.

use std::collections::BTreeMap;

use serde::Serialize;

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
