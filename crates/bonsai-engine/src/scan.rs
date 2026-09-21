use std::collections::{BTreeSet, HashMap, HashSet};
use std::num::NonZeroUsize;
use std::path::{Component, Path, PathBuf};

use bonsai_core::{Language, LanguageDescriptor};
use ignore::{DirEntry, WalkBuilder};
use tree_sitter::{Parser, Range};

use crate::config::Workspace;
use crate::finding::{normalize_key, Located};
use crate::pool;
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

#[derive(Debug)]
struct Job {
    path: PathBuf,
    shown: PathBuf,
    key_path: String,
    domain: usize,
    descriptor: &'static LanguageDescriptor,
    toplevel: bool,
}

/// A path the walk could not reach keeps its place in the list, so diagnostics stay in walk
/// order however the files around it were scored.
#[derive(Debug)]
enum Planned {
    Unreadable(String),
    Read(Job),
}

#[derive(Debug, Default)]
struct Plan {
    entries: Vec<Planned>,
    files: usize,
}

#[derive(Debug, Default)]
struct Scored {
    located: Vec<Located>,
    /// Set once the file was read: one that could not be read counts for neither the file tally
    /// nor its domain.
    domain: Option<usize>,
    warnings: Vec<String>,
    error: Option<String>,
}

struct Pass<'a> {
    workspace: &'a Workspace,
    languages: Option<&'a [String]>,
    /// `bonsai-lint src src/a.php` names `a.php` twice and must report it once.
    seen: HashSet<PathBuf>,
    plan: Plan,
}

impl Pass<'_> {
    fn walk(&mut self, given: &Path) {
        let root = ScanRoot {
            given,
            resolved: resolve(given),
        };
        for entry in WalkBuilder::new(given).build() {
            self.visit(entry, &root);
        }
    }

    fn visit(&mut self, entry: Result<DirEntry, ignore::Error>, root: &ScanRoot<'_>) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                return self
                    .plan
                    .entries
                    .push(Planned::Unreadable(error.to_string()))
            }
        };
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            return;
        }

        let Some(job) = self.job(entry.path(), root) else {
            return;
        };
        self.plan.entries.push(Planned::Read(job));
        self.plan.files += 1;
    }

    fn job(&mut self, path: &Path, root: &ScanRoot<'_>) -> Option<Job> {
        let descriptor = registry::for_path(path)?;
        let wanted = |ids: &[String]| ids.iter().any(|id| id == descriptor.spec.id);
        if self.languages.is_some_and(|ids| !wanted(ids)) {
            return None;
        }

        let absolute = root.absolute(path);
        if !self.seen.insert(absolute.clone()) || self.workspace.is_excluded(&absolute) {
            return None;
        }

        // Domain roots are absolute, so a relative scan path has to be matched absolutely or
        // every file would fall back to the root domain.
        let index = self.workspace.domain_for(&absolute);
        let domain = &self.workspace.domains[index];
        Some(Job {
            shown: display_path(path),
            key_path: normalize_key(&absolute, &domain.root),
            toplevel: domain.toplevel,
            path: path.to_path_buf(),
            descriptor,
            domain: index,
        })
    }
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
/// grammar. Parsing takes `&mut`, so a set belongs to one thread. The key is the compiled
/// language rather than the descriptor, because an embedded language picks its grammar per file.
#[derive(Default)]
struct Parsers(HashMap<usize, Parser>);

impl std::fmt::Debug for Parsers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parsers")
            .field("languages", &self.0.len())
            .finish()
    }
}

impl Parsers {
    fn analyze(
        &mut self,
        descriptor: &'static LanguageDescriptor,
        source: &str,
        toplevel: bool,
        path: &Path,
        warnings: &mut Vec<String>,
    ) -> Vec<bonsai_core::Finding> {
        let Some((language, ranges)) = region(descriptor, source, path, warnings) else {
            return Vec::new();
        };

        let parser = self
            .0
            .entry(std::ptr::from_ref(language) as usize)
            .or_insert_with(|| {
                let mut parser = Parser::new();
                parser
                    .set_language(&language.ts)
                    .expect("compiled grammar loads");
                parser
            });

        if ranges.is_empty() {
            return parse_region(parser, source, &[], language, toplevel);
        }

        // One region at a time: tree-sitter concatenates included ranges, so a line comment that
        // ends one region would run on into the next and swallow it whole.
        let mut findings = Vec::new();
        for range in &ranges {
            findings.extend(parse_region(
                parser,
                source,
                std::slice::from_ref(range),
                language,
                toplevel,
            ));
        }
        merge_toplevel(&mut findings);
        findings
    }
}

/// An empty slice restores the whole document, so a language without regions is unaffected.
fn parse_region(
    parser: &mut Parser,
    source: &str,
    ranges: &[Range],
    language: &'static Language,
    toplevel: bool,
) -> Vec<bonsai_core::Finding> {
    if parser.set_included_ranges(ranges).is_err() {
        return Vec::new();
    }

    // Partial parses still score: tree-sitter recovers from syntax errors, and a file
    // mid-edit should not blank the report.
    parser
        .parse(source, None)
        .map(|tree| bonsai_core::analyze(&tree, source.as_bytes(), language, toplevel))
        .unwrap_or_default()
}

/// A file has one top level however many regions its code is spread across. Two findings under
/// that one name would collide as a baseline key, so the regions are added up.
fn merge_toplevel(findings: &mut Vec<bonsai_core::Finding>) {
    let is_toplevel = |finding: &bonsai_core::Finding| finding.name == bonsai_core::TOPLEVEL_UNIT;
    let Some(first) = findings.iter().position(is_toplevel) else {
        return;
    };

    findings[first].score = findings
        .iter()
        .filter(|f| is_toplevel(f))
        .map(|f| f.score)
        .sum();

    let mut kept = false;
    findings.retain(|finding| {
        if !is_toplevel(finding) {
            return true;
        }
        let keep = !kept;
        kept = true;
        keep
    });
}

/// `None` when a host syntax was read but holds nothing scorable.
fn region(
    descriptor: &'static LanguageDescriptor,
    source: &str,
    path: &Path,
    warnings: &mut Vec<String>,
) -> Option<(&'static Language, Vec<Range>)> {
    let Some(extract) = descriptor.extract else {
        return Some(((descriptor.compiled)(), Vec::new()));
    };

    let extraction = extract(source);
    if let Some(warning) = extraction.warning {
        warnings.push(format!("{}: {warning}", path.display()));
    }
    (!extraction.ranges.is_empty()).then_some((extraction.language, extraction.ranges))
}

#[derive(Debug, Default)]
pub struct Scanner {
    parsers: Parsers,
    jobs: Option<NonZeroUsize>,
}

impl Scanner {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `None` asks the machine how many cores it has.
    #[must_use]
    pub fn with_jobs(mut self, jobs: Option<NonZeroUsize>) -> Self {
        self.jobs = jobs;
        self
    }

    pub fn analyze_source(
        &mut self,
        descriptor: &'static LanguageDescriptor,
        source: &str,
        toplevel: bool,
        path: &Path,
        warnings: &mut Vec<String>,
    ) -> Vec<bonsai_core::Finding> {
        self.parsers
            .analyze(descriptor, source, toplevel, path, warnings)
    }

    /// The walk is planned serially and the files are scored in parallel, then replayed in plan
    /// order: what a run prints does not depend on which worker finished first.
    pub fn scan(
        &self,
        paths: &[PathBuf],
        workspace: &Workspace,
        languages: Option<&[String]>,
    ) -> ScanOutcome {
        let plan = plan(paths, workspace, languages);
        let scored = pool::map(
            &plan.entries,
            self.workers(plan.files),
            Parsers::default,
            score,
        );

        let mut outcome = ScanOutcome::default();
        for entry in scored {
            absorb(entry, &mut outcome);
        }

        rank(&mut outcome.located);
        outcome
    }

    /// Capped by the file count, because a two-file scan has nothing to spread across eight
    /// threads, and by a small multiple of the machine, because past that the spawns cost more
    /// than the parallelism returns: uncapped, `--jobs 5000` measures slower than `--jobs 1`.
    fn workers(&self, files: usize) -> NonZeroUsize {
        let machine = std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);
        let asked = self.jobs.unwrap_or(machine);
        let ceiling = machine.get().saturating_mul(4);
        NonZeroUsize::new(files.min(asked.get()).min(ceiling)).unwrap_or(NonZeroUsize::MIN)
    }
}

fn plan(paths: &[PathBuf], workspace: &Workspace, languages: Option<&[String]>) -> Plan {
    let mut pass = Pass {
        workspace,
        languages,
        seen: HashSet::new(),
        plan: Plan::default(),
    };

    for path in paths {
        pass.walk(path);
    }
    pass.plan
}

fn score(parsers: &mut Parsers, planned: &Planned) -> Scored {
    let job = match planned {
        Planned::Read(job) => job,
        Planned::Unreadable(message) => {
            return Scored {
                error: Some(message.clone()),
                ..Scored::default()
            }
        }
    };

    let mut scored = Scored::default();
    let source = match std::fs::read(&job.path) {
        Ok(bytes) => decode(bytes, &job.shown, &mut scored.warnings),
        Err(error) => {
            scored.error = Some(format!("{}: {error}", job.shown.display()));
            return scored;
        }
    };

    scored.domain = Some(job.domain);
    scored.located = parsers
        .analyze(
            job.descriptor,
            &source,
            job.toplevel,
            &job.shown,
            &mut scored.warnings,
        )
        .into_iter()
        .map(|finding| Located {
            path: job.shown.clone(),
            key_path: job.key_path.clone(),
            domain: job.domain,
            finding,
        })
        .collect();
    scored
}

/// Replaying plan order accumulates exactly what a serial scan accumulated — counts, warnings
/// and errors alike.
fn absorb(scored: Scored, outcome: &mut ScanOutcome) {
    outcome.warnings.extend(scored.warnings);
    if let Some(message) = scored.error {
        return outcome.fail(message);
    }
    if let Some(domain) = scored.domain {
        outcome.stats.files += 1;
        outcome.stats.domains.insert(domain);
    }
    outcome.located.extend(scored.located);
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
/// walk from the current directory prepends is dropped. The separator is forward slash on every
/// platform too: these paths are read out of JSON by editors and compared in CI, where a report
/// that changes shape with the host is a report nobody can assert on.
#[must_use]
pub fn display_path(path: &Path) -> PathBuf {
    let tidy: PathBuf = path
        .components()
        .filter(|component| *component != Component::CurDir)
        .collect();
    if tidy.as_os_str().is_empty() {
        return PathBuf::from(".");
    }
    PathBuf::from(tidy.to_string_lossy().replace('\\', "/"))
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
