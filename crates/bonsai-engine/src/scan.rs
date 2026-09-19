use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use bonsai_core::LanguageDescriptor;
use ignore::WalkBuilder;
use tree_sitter::Parser;

use crate::config::Workspace;
use crate::finding::{normalize_key, Located};
use crate::registry;

#[derive(Debug, Default, Clone)]
pub struct ScanStats {
    pub files: usize,
    pub errors: usize,
    /// Which domains the scan actually visited. Stale baseline entries are only meaningful for
    /// these; a domain nobody looked at has not gone stale, it was simply out of scope.
    pub domains: BTreeSet<usize>,
}

#[derive(Debug, Default)]
pub struct ScanOutcome {
    pub located: Vec<Located>,
    pub stats: ScanStats,
    /// Paths that could not be read. Any of these makes the scan untrustworthy.
    pub errors: Vec<String>,
    /// Files that were read but not exactly as written; the scan still stands.
    pub warnings: Vec<String>,
}

/// A parser is expensive to build and `set_language` resets its state, so one is kept per
/// language rather than switching a single parser per file.
#[derive(Default)]
pub struct Scanner {
    parsers: HashMap<&'static str, Parser>,
}

impl std::fmt::Debug for Scanner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scanner")
            .field("languages", &self.parsers.len())
            .finish()
    }
}

impl Scanner {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn analyze_source(
        &mut self,
        descriptor: &'static LanguageDescriptor,
        source: &str,
        toplevel: bool,
    ) -> Vec<bonsai_core::Finding> {
        let language = (descriptor.compiled)();
        let parser = self.parsers.entry(descriptor.id).or_insert_with(|| {
            let mut parser = Parser::new();
            parser
                .set_language(&language.ts)
                .expect("compiled grammar loads");
            parser
        });

        // Partial parses still score: tree-sitter recovers from syntax errors, and a file
        // mid-edit should not blank the report.
        parser
            .parse(source, None)
            .map(|tree| bonsai_core::analyze(&tree, source.as_bytes(), language, toplevel))
            .unwrap_or_default()
    }

    pub fn scan(
        &mut self,
        paths: &[PathBuf],
        workspace: &Workspace,
        languages: Option<&[String]>,
    ) -> ScanOutcome {
        let mut outcome = ScanOutcome::default();
        let mut seen = HashSet::new();

        for path in paths {
            self.scan_one(path, workspace, languages, &mut seen, &mut outcome);
        }

        outcome.located.sort_by(|left, right| {
            right
                .finding
                .score
                .cmp(&left.finding.score)
                .then_with(|| left.path.cmp(&right.path))
                .then_with(|| left.finding.line.cmp(&right.finding.line))
        });

        outcome
    }

    fn scan_one(
        &mut self,
        root: &Path,
        workspace: &Workspace,
        languages: Option<&[String]>,
        seen: &mut HashSet<PathBuf>,
        outcome: &mut ScanOutcome,
    ) {
        let resolved_root = resolve(root);

        for entry in WalkBuilder::new(root).build() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    outcome.errors.push(error.to_string());
                    outcome.stats.errors += 1;
                    continue;
                }
            };

            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }

            let path = entry.path();
            let Some(descriptor) = registry::for_path(path) else {
                continue;
            };

            if languages.is_some_and(|wanted| !wanted.iter().any(|id| id == descriptor.spec.id)) {
                continue;
            }

            // Pure path arithmetic per file; only the scan root paid for a `canonicalize`.
            let absolute = path
                .strip_prefix(root)
                .map_or_else(|_| resolve(path), |relative| resolved_root.join(relative));

            // `bonsai-lint src src/a.php` names `a.php` twice and must report it once.
            if !seen.insert(absolute.clone()) {
                continue;
            }

            if workspace.is_excluded(&absolute) {
                continue;
            }

            let source = match std::fs::read(path) {
                Ok(bytes) => decode(bytes, path, &mut outcome.warnings),
                Err(error) => {
                    outcome.errors.push(format!("{}: {error}", path.display()));
                    outcome.stats.errors += 1;
                    continue;
                }
            };

            outcome.stats.files += 1;

            // Domain roots are absolute, so a relative scan path has to be matched absolutely
            // or every file would fall back to the root domain.
            let index = workspace.domain_for(&absolute);
            outcome.stats.domains.insert(index);
            let domain = &workspace.domains[index];
            let key_path = normalize_key(&absolute, &domain.root);

            for finding in self.analyze_source(descriptor, &source, domain.toplevel) {
                outcome.located.push(Located {
                    path: path.to_path_buf(),
                    key_path: key_path.clone(),
                    domain: index,
                    finding,
                });
            }
        }
    }
}

/// Legacy Latin-1 sources still parse; the replaced bytes sit in strings and comments, which do
/// not score, so the file is decoded leniently and the loss reported.
pub fn decode(bytes: Vec<u8>, path: &Path, warnings: &mut Vec<String>) -> String {
    String::from_utf8(bytes).unwrap_or_else(|error| {
        warnings.push(format!(
            "{}: not valid UTF-8, undecodable bytes were replaced",
            path.display()
        ));
        String::from_utf8_lossy(error.as_bytes()).into_owned()
    })
}

/// Every path that is compared against a domain root goes through here, so `/tmp` and
/// `/private/tmp` cannot end up on opposite sides of a `starts_with`. A file that does not exist
/// yet, an unsaved editor buffer, resolves through its directory.
#[must_use]
pub fn resolve(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    match (absolute.parent(), absolute.file_name()) {
        (Some(parent), Some(name)) => parent
            .canonicalize()
            .map_or(absolute.clone(), |parent| parent.join(name)),
        _ => absolute,
    }
}
