use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use bonsai_core::LanguageDescriptor;
use ignore::{DirEntry, WalkBuilder};
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

impl ScanOutcome {
    fn fail(&mut self, message: String) {
        self.errors.push(message);
        self.stats.errors += 1;
    }
}

/// What one `scan` call accumulates across all its paths.
struct Pass<'a> {
    workspace: &'a Workspace,
    languages: Option<&'a [String]>,
    /// `bonsai-lint src src/a.php` names `a.php` twice and must report it once.
    seen: HashSet<PathBuf>,
    outcome: ScanOutcome,
}

/// Pure path arithmetic per file; only the scan root paid for a `canonicalize`.
struct ScanRoot<'a> {
    given: &'a Path,
    resolved: PathBuf,
}

impl ScanRoot<'_> {
    fn absolute(&self, path: &Path) -> PathBuf {
        path.strip_prefix(self.given)
            .map_or_else(|_| resolve(path), |relative| self.resolved.join(relative))
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
    ) -> ScanOutcome {
        let mut pass = Pass {
            workspace,
            languages,
            seen: HashSet::new(),
            outcome: ScanOutcome::default(),
        };

        for path in paths {
            self.scan_one(path, &mut pass);
        }

        rank(&mut pass.outcome.located);
        pass.outcome
    }

    fn scan_one(&mut self, given: &Path, pass: &mut Pass<'_>) {
        let root = ScanRoot {
            given,
            resolved: resolve(given),
        };
        for entry in WalkBuilder::new(given).build() {
            self.scan_entry(entry, &root, pass);
        }
    }

    fn scan_entry(
        &mut self,
        entry: Result<DirEntry, ignore::Error>,
        root: &ScanRoot<'_>,
        pass: &mut Pass<'_>,
    ) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => return pass.outcome.fail(error.to_string()),
        };
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            return;
        }

        let path = entry.path();
        let Some(descriptor) = registry::for_path(path) else {
            return;
        };
        let wanted = |ids: &[String]| ids.iter().any(|id| id == descriptor.spec.id);
        if pass.languages.is_some_and(|ids| !wanted(ids)) {
            return;
        }

        let absolute = root.absolute(path);
        if !pass.seen.insert(absolute.clone()) || pass.workspace.is_excluded(&absolute) {
            return;
        }

        let shown = display_path(path);
        let source = match std::fs::read(path) {
            Ok(bytes) => decode(bytes, &shown, &mut pass.outcome.warnings),
            Err(error) => return pass.outcome.fail(format!("{}: {error}", shown.display())),
        };
        pass.outcome.stats.files += 1;

        // Domain roots are absolute, so a relative scan path has to be matched absolutely or
        // every file would fall back to the root domain.
        let index = pass.workspace.domain_for(&absolute);
        pass.outcome.stats.domains.insert(index);
        let domain = &pass.workspace.domains[index];
        let key_path = normalize_key(&absolute, &domain.root);

        for finding in self.analyze_source(descriptor, &source, domain.toplevel) {
            pass.outcome.located.push(Located {
                path: shown.clone(),
                key_path: key_path.clone(),
                domain: index,
                finding,
            });
        }
    }
}

/// Highest score first, then by position, so the worst offender heads every report whichever
/// way the files were read.
pub fn rank(located: &mut [Located]) {
    located.sort_by(|left, right| {
        right
            .finding
            .score
            .cmp(&left.finding.score)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.finding.line.cmp(&right.finding.line))
    });
}

/// `bonsai-lint .` and `bonsai-lint src` must print `src/a.php` the same way, so the `./` a
/// walk from the current directory prepends is dropped.
#[must_use]
pub fn display_path(path: &Path) -> PathBuf {
    let tidy: PathBuf = path
        .components()
        .filter(|component| *component != Component::CurDir)
        .collect();
    if tidy.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        tidy
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
