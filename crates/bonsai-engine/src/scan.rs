use std::collections::{BTreeSet, HashMap};
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

impl ScanStats {
    fn absorb(&mut self, other: Self) {
        self.files += other.files;
        self.errors += other.errors;
        self.domains.extend(other.domains);
    }
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
    ) -> (Vec<Located>, ScanStats, Vec<String>) {
        let mut located = Vec::new();
        let mut stats = ScanStats::default();
        let mut errors = Vec::new();

        for path in paths {
            let partial = self.scan_one(path, workspace, languages, &mut located, &mut errors);
            stats.absorb(partial);
        }

        located.sort_by(|left, right| {
            right
                .finding
                .score
                .cmp(&left.finding.score)
                .then_with(|| left.path.cmp(&right.path))
                .then_with(|| left.finding.line.cmp(&right.finding.line))
        });

        (located, stats, errors)
    }

    fn scan_one(
        &mut self,
        root: &Path,
        workspace: &Workspace,
        languages: Option<&[String]>,
        located: &mut Vec<Located>,
        errors: &mut Vec<String>,
    ) -> ScanStats {
        let mut stats = ScanStats::default();

        for entry in WalkBuilder::new(root).build() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    errors.push(error.to_string());
                    stats.errors += 1;
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

            if languages.is_some_and(|wanted| !wanted.iter().any(|id| id == descriptor.id)) {
                continue;
            }

            // `absolute` is pure path arithmetic; `canonicalize` would be a syscall per file,
            // which on a large repository costs more than the parsing does.
            let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());

            if workspace.is_excluded(&absolute) {
                continue;
            }

            let source = match std::fs::read_to_string(path) {
                Ok(source) => source,
                Err(error) => {
                    errors.push(format!("{}: {error}", path.display()));
                    stats.errors += 1;
                    continue;
                }
            };

            stats.files += 1;

            // Scan paths are usually relative while domain roots are absolute, so matching has
            // to happen on an absolute path or every file would fall back to the root domain.
            let index = workspace.domain_for(&absolute);
            stats.domains.insert(index);
            let domain = &workspace.domains[index];
            let key_path = normalize_key(&absolute, &domain.root);

            for finding in self.analyze_source(descriptor, &source, domain.toplevel) {
                located.push(Located {
                    path: path.to_path_buf(),
                    key_path: key_path.clone(),
                    domain: index,
                    finding,
                });
            }
        }

        stats
    }
}
