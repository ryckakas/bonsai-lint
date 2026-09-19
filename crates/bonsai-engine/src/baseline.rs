use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::finding::Located;

const FORMAT_VERSION: u32 = 1;

/// Recorded scores for code that already exists, so an established codebase can adopt a
/// threshold without fixing everything first.
#[derive(Debug, Serialize, Deserialize)]
pub struct Baseline {
    version: u32,
    entries: BTreeMap<String, BTreeMap<String, u32>>,
}

impl Default for Baseline {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            entries: BTreeMap::new(),
        }
    }
}

impl Baseline {
    #[must_use]
    pub fn from_findings(findings: &[Located]) -> Self {
        let mut entries: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();

        for located in findings {
            entries
                .entry(located.key_path.clone())
                .or_default()
                .insert(located.finding.qualified_name(), located.finding.score);
        }

        Self {
            version: FORMAT_VERSION,
            entries,
        }
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let text = fs::read_to_string(path)?;
        let baseline: Self = serde_json::from_str(&text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

        if baseline.version != FORMAT_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "baseline format version {} is not supported (expected {FORMAT_VERSION}); \
                     regenerate with --write-baseline",
                    baseline.version
                ),
            ));
        }

        Ok(baseline)
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let mut text = serde_json::to_string_pretty(self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        text.push('\n');
        fs::write(path, text)
    }

    /// An unknown key is a regression, not an acceptance. A renamed or re-keyed unit therefore
    /// fails loud rather than silently inheriting someone else's grandfathering.
    #[must_use]
    pub fn is_regression(&self, located: &Located) -> bool {
        self.entries
            .get(&located.key_path)
            .and_then(|units| units.get(&located.finding.qualified_name()))
            .is_none_or(|accepted| located.finding.score > *accepted)
    }

    /// Entries that matched nothing in this scan. Stale keys are how a baseline rots into a
    /// permanent amnesty, so they are surfaced rather than left to accumulate.
    #[must_use]
    pub fn unmatched(&self, findings: &[Located]) -> Vec<String> {
        let seen: BTreeSet<(&str, String)> = findings
            .iter()
            .map(|located| (located.key_path.as_str(), located.finding.qualified_name()))
            .collect();

        self.entries
            .iter()
            .flat_map(|(path, units)| units.keys().map(move |unit| (path, unit)))
            .filter(|(path, unit)| !seen.contains(&(path.as_str(), (*unit).clone())))
            .map(|(path, unit)| format!("{path}: {unit}"))
            .collect()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.values().map(BTreeMap::len).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
