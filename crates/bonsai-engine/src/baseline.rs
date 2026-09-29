//! Baselines, which record the findings already over a threshold so only new or worse ones fail.

use std::collections::{BTreeMap, HashMap};
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
    /// Two units the namer cannot tell apart share one entry, which keeps the higher score so an
    /// unchanged rerun accepts both.
    #[must_use]
    pub fn from_findings(findings: &[Located]) -> Self {
        let mut entries: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();

        for located in findings {
            let score = located.finding.score;
            entries
                .entry(located.key_path.clone())
                .or_default()
                .entry(located.finding.qualified_name())
                .and_modify(|accepted| *accepted = (*accepted).max(score))
                .or_insert(score);
        }

        Self {
            version: FORMAT_VERSION,
            entries,
        }
    }

    /// Reads a baseline written by [`Baseline::save`].
    ///
    /// A file that does not parse, or holds another format version, fails with
    /// [`io::ErrorKind::InvalidData`] rather than loading as an empty baseline.
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

    /// Writes the baseline as pretty-printed JSON, creating any missing parent directories.
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

    /// Returns the entries a rewrite over the same scan would drop or lower, in path and unit order.
    ///
    /// `located` is every unit the scan scored for this baseline, suppressed and passing ones
    /// included, and `threshold` gives the threshold each unit is held to. Units sharing an entry
    /// are judged by the higher score, as [`Baseline::from_findings`] records it, and a unit
    /// suppressed with a reason is never recorded, so it counts only when nothing else matches.
    #[must_use]
    pub fn loose<'a>(
        &self,
        located: impl IntoIterator<Item = &'a Located>,
        threshold: impl Fn(&Located) -> u32,
    ) -> Vec<LooseEntry> {
        let mut by_key: HashMap<(&str, String), Vec<&Located>> = HashMap::new();
        for item in located {
            by_key
                .entry((item.key_path.as_str(), item.finding.qualified_name()))
                .or_default()
                .push(item);
        }

        self.entries
            .iter()
            .flat_map(|(path, units)| {
                units
                    .iter()
                    .map(move |(unit, &recorded)| (path, unit, recorded))
            })
            .filter_map(|(path, unit, recorded)| {
                let matches = by_key
                    .get(&(path.as_str(), unit.clone()))
                    .map_or(&[][..], Vec::as_slice);
                let reason = judge(recorded, matches, &threshold)?;
                Some(LooseEntry {
                    key_path: path.clone(),
                    unit: unit.clone(),
                    recorded,
                    reason,
                })
            })
            .collect()
    }

    /// Returns how many units the baseline accepts, across every file.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.values().map(BTreeMap::len).sum()
    }

    /// Returns whether the baseline accepts no unit at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A baseline entry that a rewrite over the same scan would drop or lower.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LooseEntry {
    /// The file the entry is keyed under, as the baseline records it.
    pub key_path: String,
    /// The unit's qualified name, as the baseline records it.
    pub unit: String,
    /// The score the baseline accepts.
    pub recorded: u32,
    /// Why the entry is looser than the code.
    pub reason: Looseness,
}

/// Why a baseline entry is looser than the code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Looseness {
    /// No unit in the scan has the entry's key, as after a deletion or a rename.
    Unmatched,
    /// Every unit with the entry's key carries a suppression marker with a reason.
    Suppressed,
    /// The unit no longer scores over its threshold, so it needs no entry.
    AtOrUnderThreshold {
        /// The unit's score, the higher one where two units share the entry.
        score: u32,
        /// The threshold that unit is held to.
        threshold: u32,
    },
    /// The unit is still over its threshold, but scores less than the entry accepts.
    Lower {
        /// The unit's score, the higher one where two units share the entry.
        score: u32,
    },
}

fn judge(
    recorded: u32,
    matches: &[&Located],
    threshold: &impl Fn(&Located) -> u32,
) -> Option<Looseness> {
    if matches.is_empty() {
        return Some(Looseness::Unmatched);
    }
    let unsuppressed = matches.iter().filter(|item| !item.finding.is_suppressed());
    let Some(highest) = unsuppressed.clone().max_by_key(|item| item.finding.score) else {
        return Some(Looseness::Suppressed);
    };
    let over = unsuppressed
        .filter(|item| item.finding.score > threshold(item))
        .map(|item| item.finding.score)
        .max();
    match over {
        None => Some(Looseness::AtOrUnderThreshold {
            score: highest.finding.score,
            threshold: threshold(highest),
        }),
        Some(score) if score < recorded => Some(Looseness::Lower { score }),
        Some(_) => None,
    }
}
