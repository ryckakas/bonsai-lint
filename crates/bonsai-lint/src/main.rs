//! The command line of bonsai-lint, a multi-language cognitive complexity linter.

mod github;

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bonsai_core::Suppression;
use bonsai_engine::config::{self, Workspace};
use bonsai_engine::finding::normalize_key;
use bonsai_engine::report::{Report, ReportedFinding, ReportedLooseEntry};
use bonsai_engine::scan::{decode, display_path, rank, resolve};
use bonsai_engine::{
    Baseline, Located, LooseEntry, Looseness, STACK_SIZE, ScanOutcome, ScanStats, Scanner, registry,
};
use clap::error::ErrorKind;
use clap::{CommandFactory, FromArgMatches, Parser as ClapParser, ValueEnum};

use crate::github::Level;

// musl's malloc serialises threads on one lock, which made a parallel scan over 15 times slower
// than glibc's. The `override` feature is the half that matters: tree-sitter's C calls malloc.
#[cfg(target_env = "musl")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(ClapParser)]
#[command(name = "bonsai-lint", version, about = "Cognitive complexity linter")]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each bool is its own command-line flag"
)]
struct Args {
    /// Files or directories to scan
    #[arg(default_value = ".")]
    paths: Vec<PathBuf>,

    /// Complexity threshold; `15` for everything, or `php=15,typescript=20` per language
    #[arg(long, value_name = "N|LANG=N")]
    over: Vec<String>,

    /// Print every unit, not just those above the threshold
    #[arg(long)]
    all: bool,

    /// Output format; `json` is what editors read, `github` annotates pull requests
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,

    /// Record current findings as accepted, so only later regressions fail
    #[arg(long)]
    write_baseline: bool,

    /// Use one baseline file, keyed from the workspace root, instead of each domain's own
    #[arg(long, value_name = "PATH")]
    baseline: Option<PathBuf>,

    /// Fail when a baseline entry is looser than the code
    #[arg(long)]
    strict_baseline: bool,

    /// Restrict the scan to these languages
    #[arg(long, value_delimiter = ',', value_name = "ID")]
    lang: Vec<String>,

    /// Restrict the scan to one named domain
    #[arg(long, value_name = "NAME")]
    domain: Option<String>,

    /// Start config discovery here instead of the first scanned path
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Succeed instead of failing when no supported file is found
    #[arg(long)]
    allow_no_files: bool,

    /// Do not score code outside any function
    #[arg(long)]
    no_toplevel: bool,

    /// Read one file's contents from stdin, so unsaved editor buffers can be analysed
    #[arg(long)]
    stdin: bool,

    /// The path the stdin contents should be attributed to
    #[arg(long, value_name = "PATH", requires = "stdin")]
    stdin_path: Option<PathBuf>,

    /// How many files to score at once; `0` asks the machine
    #[arg(short, long, value_name = "N", default_value_t = 0)]
    jobs: usize,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum Format {
    Text,
    Json,
    Github,
}

fn main() -> ExitCode {
    let args = parse_args();
    let format = args.format;

    // The engine gives its own workers this stack; `--stdin` parses on this thread instead.
    let outcome = std::thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(move || run(&args))
        .map_err(|error| format!("could not start the scan: {error}"))
        .and_then(|worker| worker.join().map_err(|_| "the scan aborted".to_string()));

    match outcome {
        Ok(Ok(code)) => code,
        Ok(Err(message)) | Err(message) => {
            say(format, Level::Error, &message);
            ExitCode::FAILURE
        }
    }
}

/// The `--lang` help names what the registry holds, so a build with fewer languages never offers
/// one it cannot scan.
fn parse_args() -> Args {
    let ids = registry::language_ids();
    let offered = if ids.is_empty() {
        "none are built into this binary".to_string()
    } else {
        ids.iter()
            .map(|id| format!("`{id}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut command = Args::command().mut_arg("lang", |arg| {
        arg.help(format!("Restrict the scan to these languages ({offered})"))
    });
    let matches = command.get_matches_mut();
    let args =
        Args::from_arg_matches(&matches).unwrap_or_else(|error| error.format(&mut command).exit());
    // Annotating every unit that passes would bury the ones that fail.
    if args.all && args.format == Format::Github {
        command
            .error(
                ErrorKind::ArgumentConflict,
                "--all cannot be combined with --format github",
            )
            .exit();
    }
    args
}

/// Prints a message about the run as a whole, which `github` shows in the run's summary.
fn say(format: Format, level: Level, message: &str) {
    if format == Format::Github {
        eprintln!("{}", github::annotation(level, message));
    } else {
        eprintln!("{message}");
    }
}

fn run(args: &Args) -> Result<ExitCode, String> {
    let known = registry::language_ids();
    reject_unknown_languages(args, &known)?;

    let start = args
        .config
        .clone()
        .or_else(|| args.paths.first().cloned())
        .unwrap_or_else(|| PathBuf::from("."));
    let start = resolve(&start);

    let mut workspace = config::discover(&start, &known).map_err(|error| error.to_string())?;
    apply_overrides(args, &mut workspace)?;

    let domain_filter = args
        .domain
        .as_ref()
        .map(|wanted| {
            workspace
                .domains
                .iter()
                .position(|domain| &domain.name == wanted)
                .ok_or_else(|| format!("no domain named `{wanted}`"))
        })
        .transpose()?;

    let scan_roots: Vec<PathBuf> = args.paths.iter().map(|path| resolve(path)).collect();

    for warning in &workspace.warnings {
        say(args.format, Level::Warning, warning);
    }
    if !args.stdin {
        for warning in workspace.undeclared_config_warnings(&scan_roots) {
            say(args.format, Level::Warning, &warning);
        }
    }

    let mut scanner = Scanner::new().with_jobs(NonZeroUsize::new(args.jobs));
    let languages = (!args.lang.is_empty()).then(|| args.lang.clone());

    let ScanOutcome {
        mut located,
        stats,
        errors,
        warnings,
    } = if args.stdin {
        scan_stdin(args, &workspace, &mut scanner)?
    } else {
        scanner.scan(&args.paths, &workspace, languages.as_deref())
    };

    print_diagnostics(args.format, &warnings, &errors);

    if let Some(index) = domain_filter {
        located.retain(|item| item.domain == index);
    }

    // A gate that could not read what it was pointed at must not report success; otherwise a
    // renamed directory turns the check into a no-op that stays green.
    if let Some(failure) = unusable_scan(&stats, args) {
        return Err(failure);
    }

    if args.baseline.is_some() {
        rekey_to_workspace(&mut located, &workspace);
    }

    warn_about_unreasoned_suppressions(&located, &workspace, args.format);

    let over_threshold: Vec<Located> = located
        .iter()
        .filter(|item| is_over_threshold(item, &workspace))
        .cloned()
        .collect();

    if args.write_baseline {
        return write_baselines(&over_threshold, args, &workspace);
    }

    let baselines = load_baselines(args, &workspace)?;
    let (loose, held) = report_loose_entries(&baselines, &located, &workspace, args, &scan_roots);
    let strict_loose = loose.iter().filter(|entry| entry.strict).count();

    let breaches: Vec<&Located> = over_threshold
        .iter()
        .filter(|item| is_regression(item, &baselines, args))
        .collect();

    let gated_by = domain_filter.unwrap_or_else(|| only_visited_domain(&stats));
    let printed = match args.format {
        Format::Text => print_text(&located, args, &workspace, &baselines),
        Format::Github => print_github(&located, args, &workspace, &baselines, &held),
        Format::Json => print_json(
            &located,
            args,
            &workspace,
            &baselines,
            breaches.len(),
            gated_by,
            loose,
        ),
    };
    tolerate_closed_pipe(printed)?;

    let accepted = over_threshold.len() - breaches.len();
    let partial = !scan_roots
        .iter()
        .any(|path| workspace.root.starts_with(path));
    Ok(verdict(
        breaches.len(),
        accepted,
        strict_loose,
        partial,
        args,
    ))
}

/// A strict baseline fails on entries looser than the code, but the hint must never teach anyone
/// to rewrite the baseline while something may still fail, here or in a part of the workspace
/// this run did not see, because the rewrite would accept that too.
fn verdict(
    breaches: usize,
    accepted: usize,
    strict_loose: usize,
    partial: bool,
    args: &Args,
) -> ExitCode {
    if breaches == 0 && strict_loose == 0 {
        return ExitCode::SUCCESS;
    }
    if args.format != Format::Json {
        eprintln!();
        if breaches > 0 {
            eprintln!("{}", breach_summary(breaches, accepted));
        }
        if strict_loose > 0 {
            eprintln!("{}", tighten_hint(strict_loose, breaches > 0 || partial));
        }
    }
    let errors = breaches + strict_loose;
    if args.format == Format::Github && errors > github::ERRORS_PER_STEP {
        let note = format!(
            "GitHub annotates at most {} errors per step; all {errors} are listed in the log",
            github::ERRORS_PER_STEP
        );
        eprintln!("{}", github::annotation(Level::Notice, &note));
    }
    ExitCode::FAILURE
}

fn breach_summary(breaches: usize, accepted: usize) -> String {
    if accepted == 0 {
        return format!("{breaches} unit(s) over the threshold");
    }
    format!("{breaches} unit(s) over the threshold, {accepted} accepted by a baseline")
}

fn tighten_hint(strict_loose: usize, unsure: bool) -> String {
    let when = if unsure {
        "once nothing else fails, "
    } else {
        ""
    };
    format!(
        "{strict_loose} entr(ies) of a strict baseline looser than the code; {when}tighten them \
         with --write-baseline over the whole workspace, with this run's other flags"
    )
}

fn reject_unknown_languages(args: &Args, known: &[&str]) -> Result<(), String> {
    let overridden = args
        .over
        .iter()
        .flat_map(|value| value.split(','))
        .filter_map(|part| part.split_once('=').map(|(language, _)| language.trim()));

    for id in args.lang.iter().map(String::as_str).chain(overridden) {
        if !known.contains(&id) {
            return Err(format!(
                "unknown language `{id}`; expected one of: {}",
                known.join(", ")
            ));
        }
    }
    Ok(())
}

fn scan_stdin(
    args: &Args,
    workspace: &Workspace,
    scanner: &mut Scanner,
) -> Result<ScanOutcome, String> {
    let path = args
        .stdin_path
        .clone()
        .ok_or_else(|| "--stdin needs --stdin-path to know which language to use".to_string())?;

    let descriptor = registry::for_path(&path)
        .ok_or_else(|| format!("{}: no language handles this extension", path.display()))?;

    let mut bytes = Vec::new();
    io::stdin()
        .read_to_end(&mut bytes)
        .map_err(|error| format!("reading stdin: {error}"))?;

    let absolute = resolve(&path);
    let index = workspace.domain_for(&absolute);
    let domain = &workspace.domains[index];

    let mut outcome = ScanOutcome {
        stats: ScanStats {
            files: 1,
            errors: 0,
            domains: std::iter::once(index).collect(),
        },
        ..ScanOutcome::default()
    };

    // The editor must agree with CI, and CI never sees an excluded file.
    if workspace.is_excluded(&absolute) {
        return Ok(outcome);
    }

    let mut decoding = Vec::new();
    let source = decode(bytes, &path, &mut decoding);
    if descriptor.is_generated(&source) {
        return Ok(outcome);
    }
    outcome.warnings.extend(decoding);
    let key_path = normalize_key(&absolute, &domain.root);

    let shown = display_path(&path);
    let findings = scanner.analyze_source(
        descriptor,
        &source,
        domain.toplevel,
        &shown,
        &mut outcome.warnings,
    );
    outcome.located = findings
        .into_iter()
        .map(|finding| Located {
            path: shown.clone(),
            key_path: key_path.clone(),
            domain: index,
            finding,
        })
        .collect();
    rank(&mut outcome.located);

    Ok(outcome)
}

/// The thresholds a report claims must be the ones the scan was gated by. That is one answer
/// for an editor buffer or a single package; a scan across domains falls back to the root's.
fn only_visited_domain(stats: &ScanStats) -> usize {
    match stats.domains.iter().copied().collect::<Vec<_>>()[..] {
        [only] => only,
        _ => 0,
    }
}

fn apply_overrides(args: &Args, workspace: &mut Workspace) -> Result<(), String> {
    let mut global: Option<u32> = None;
    let mut per_language: BTreeMap<String, u32> = BTreeMap::new();

    for part in args.over.iter().flat_map(|value| value.split(',')) {
        match part.split_once('=') {
            Some((language, number)) => {
                per_language.insert(language.trim().to_string(), parse_threshold(number)?);
            }
            None => global = Some(parse_threshold(part)?),
        }
    }

    for domain in &mut workspace.domains {
        if let Some(threshold) = global {
            domain.threshold = threshold;
            domain.language_thresholds.clear();
        }
        for (language, threshold) in &per_language {
            domain
                .language_thresholds
                .insert(language.clone(), *threshold);
        }
        if args.no_toplevel {
            domain.toplevel = false;
        }
        if args.strict_baseline {
            domain.strict_baseline = true;
        }
    }

    Ok(())
}

fn parse_threshold(text: &str) -> Result<u32, String> {
    text.trim()
        .parse()
        .map_err(|_| format!("--over: `{text}` is not a number"))
}

/// One shared baseline spans every domain, so its keys are workspace-relative; two domains each
/// holding a `src/index.ts` would otherwise fight over one entry.
fn rekey_to_workspace(located: &mut [Located], workspace: &Workspace) {
    for item in located {
        let domain = &workspace.domains[item.domain];
        item.key_path = normalize_key(&domain.root.join(&item.key_path), &workspace.root);
    }
}

/// Writing a baseline, counting breaches and judging a baseline's entries must agree on what a
/// unit is held to, or a freshly written baseline could read as loose.
fn threshold_of(item: &Located, workspace: &Workspace) -> u32 {
    workspace.domains[item.domain].threshold_for(item.finding.language)
}

fn is_over_threshold(item: &Located, workspace: &Workspace) -> bool {
    item.finding.score > threshold_of(item, workspace) && !item.finding.is_suppressed()
}

fn baseline_index(item: &Located, args: &Args) -> usize {
    if args.baseline.is_some() {
        0
    } else {
        item.domain
    }
}

fn is_regression(item: &Located, baselines: &BTreeMap<usize, Baseline>, args: &Args) -> bool {
    baselines
        .get(&baseline_index(item, args))
        .is_none_or(|baseline| baseline.is_regression(item))
}

fn is_reported(
    item: &Located,
    args: &Args,
    workspace: &Workspace,
    baselines: &BTreeMap<usize, Baseline>,
) -> bool {
    is_over_threshold(item, workspace) && is_regression(item, baselines, args)
}

fn load_baselines(args: &Args, workspace: &Workspace) -> Result<BTreeMap<usize, Baseline>, String> {
    let mut loaded = BTreeMap::new();

    if let Some(path) = &args.baseline {
        if path.exists() {
            let baseline =
                Baseline::load(path).map_err(|error| format!("{}: {error}", path.display()))?;
            loaded.insert(0, baseline);
        }
        return Ok(loaded);
    }

    for (index, domain) in workspace.domains.iter().enumerate() {
        if domain.baseline.exists() {
            let baseline = Baseline::load(&domain.baseline)
                .map_err(|error| format!("{}: {error}", domain.baseline.display()))?;
            loaded.insert(index, baseline);
        }
    }

    Ok(loaded)
}

fn write_baselines(
    over_threshold: &[Located],
    args: &Args,
    workspace: &Workspace,
) -> Result<ExitCode, String> {
    if let Some(path) = &args.baseline {
        let baseline = Baseline::from_findings(over_threshold);
        baseline
            .save(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        report_written(&baseline, path)?;
        return Ok(ExitCode::SUCCESS);
    }

    let mut by_domain: BTreeMap<usize, Vec<Located>> = BTreeMap::new();
    for item in over_threshold {
        by_domain.entry(item.domain).or_default().push(item.clone());
    }

    let mut written = 0;
    for (index, domain) in workspace.domains.iter().enumerate() {
        let items = by_domain.remove(&index).unwrap_or_default();
        if items.is_empty() && !domain.baseline.exists() {
            continue;
        }
        let baseline = Baseline::from_findings(&items);
        baseline
            .save(&domain.baseline)
            .map_err(|error| format!("{}: {error}", domain.baseline.display()))?;
        report_written(&baseline, &domain.baseline)?;
        written += 1;
    }

    // Silence here would read the same as a wrong path or a wrong config.
    if written == 0 {
        tolerate_closed_pipe(writeln!(
            io::stdout().lock(),
            "nothing above the threshold; no baseline written"
        ))?;
    }

    Ok(ExitCode::SUCCESS)
}

fn report_written(baseline: &Baseline, path: &Path) -> Result<(), String> {
    tolerate_closed_pipe(writeln!(
        io::stdout().lock(),
        "recorded {} finding(s) in {}",
        baseline.len(),
        path.display()
    ))
}

/// Only a scan that saw a whole domain can tell that an entry has gone stale. One editor buffer,
/// one language or one sub-directory would report everything else as stale.
fn report_loose_entries(
    baselines: &BTreeMap<usize, Baseline>,
    located: &[Located],
    workspace: &Workspace,
    args: &Args,
    scan_roots: &[PathBuf],
) -> (Vec<ReportedLooseEntry>, Vec<String>) {
    let partial = args.stdin || args.domain.is_some() || !args.lang.is_empty();
    let covered = |root: &Path| !partial && scan_roots.iter().any(|path| root.starts_with(path));
    let shared = args.baseline.is_some();

    let mut loose = Vec::new();
    let mut held = Vec::new();
    let mut unjudged = 0;
    for (index, baseline) in baselines {
        let domain = &workspace.domains[*index];
        let (name, root) = if shared {
            ("baseline", workspace.root.as_path())
        } else {
            (domain.name.as_str(), domain.root.as_path())
        };
        let file = args.baseline.as_deref().unwrap_or(&domain.baseline);
        if !covered(root) {
            unjudged += 1;
            continue;
        }

        let scoped = located
            .iter()
            .filter(|item| shared || item.domain == *index);
        for entry in baseline.loose(scoped, |item| threshold_of(item, workspace)) {
            held.extend(announce_loose(
                args.format,
                name,
                &entry,
                file,
                domain.strict_baseline,
            ));
            let owner = (!shared).then(|| name.to_string());
            loose.push(ReportedLooseEntry::new(
                owner,
                &entry,
                domain.strict_baseline,
            ));
        }
    }

    if args.strict_baseline && unjudged > 0 {
        let note = format!(
            "--strict-baseline: {unjudged} baseline(s) not judged; only a scan of a whole domain, \
             without --stdin, --domain or --lang, judges its baseline"
        );
        say(args.format, Level::Warning, &note);
    }
    (loose, held)
}

/// A loose entry in a strict baseline fails the run, so `github` makes it an error on the
/// baseline file, and holds it back until the breaches are out: the runner annotates only the
/// first ten errors, and a breach on the pull request's own lines must not lose its place.
fn announce_loose(
    format: Format,
    owner: &str,
    entry: &LooseEntry,
    baseline: &Path,
    strict: bool,
) -> Option<String> {
    let message = format!(
        "{owner}: {}: {} is baselined at {} but {}",
        entry.key_path,
        entry.unit,
        entry.recorded,
        describe(entry.reason)
    );
    if format != Format::Github {
        eprintln!("{message}");
        return None;
    }
    let level = if strict { Level::Error } else { Level::Warning };
    Some(github::annotation_on(level, baseline, &message))
}

fn describe(reason: Looseness) -> String {
    match reason {
        Looseness::Unmatched => "matched nothing in this scan".to_string(),
        Looseness::Suppressed => "is suppressed".to_string(),
        Looseness::NotOverThreshold { score, threshold } => {
            format!("scores {score}, not over its threshold of {threshold}")
        }
        Looseness::Lower { score } => format!("scores {score}"),
    }
}

/// A marker without a reason is refused rather than obeyed, so it has to say so out loud —
/// otherwise the author would believe the finding was silenced.
fn warn_about_unreasoned_suppressions(located: &[Located], workspace: &Workspace, format: Format) {
    let unreasoned = located.iter().filter(|item| {
        item.finding.score > threshold_of(item, workspace)
            && item.finding.suppression == Suppression::MissingReason
    });
    for item in unreasoned {
        let message = format!(
            "`{}` needs a reason, e.g. `// {}: why this has to stay complex` — ignoring it and \
             reporting {}",
            bonsai_core::SUPPRESSION_MARKER,
            bonsai_core::SUPPRESSION_MARKER,
            item.finding.qualified_name()
        );
        if format == Format::Github {
            let line = item.finding.line;
            eprintln!(
                "{}",
                github::annotation_at(Level::Warning, &item.path, line, &message)
            );
        } else {
            eprintln!("{}:{}: {message}", item.path.display(), item.finding.line);
        }
    }
}

/// A file that could not be read fails the run, so `github` shows why; a file decoded leniently
/// only warns, and stays in the log so a legacy tree cannot use up the annotations.
fn print_diagnostics(format: Format, warnings: &[String], errors: &[String]) {
    for warning in warnings {
        // A file name is data; in `github` a line break in one must not start a command.
        if format == Format::Github {
            eprintln!("{}", github::escape_data(warning));
        } else {
            eprintln!("{warning}");
        }
    }
    for error in errors {
        say(format, Level::Error, error);
    }
}

fn unusable_scan(stats: &ScanStats, args: &Args) -> Option<String> {
    if stats.errors > 0 {
        return Some(format!(
            "{} path(s) could not be read; refusing to report a clean run",
            stats.errors
        ));
    }

    if stats.files == 0 && !args.allow_no_files {
        let paths: Vec<String> = args
            .paths
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        return Some(format!(
            "no supported files found under {}",
            paths.join(", ")
        ));
    }

    None
}

/// `bonsai-lint --all . | head` closes the pipe early; that is the reader's choice, not a failure.
fn tolerate_closed_pipe(result: io::Result<()>) -> Result<(), String> {
    match result {
        Err(error) if error.kind() != io::ErrorKind::BrokenPipe => {
            Err(format!("writing output: {error}"))
        }
        _ => Ok(()),
    }
}

fn print_text(
    located: &[Located],
    args: &Args,
    workspace: &Workspace,
    baselines: &BTreeMap<usize, Baseline>,
) -> io::Result<()> {
    let mut out = io::stdout().lock();
    for item in located {
        if args.all || is_reported(item, args, workspace, baselines) {
            writeln!(
                out,
                "{:>4}  {}:{}  {}",
                item.finding.score,
                item.path.display(),
                item.finding.line,
                item.finding.qualified_name()
            )?;
        }
    }
    Ok(())
}

fn print_github(
    located: &[Located],
    args: &Args,
    workspace: &Workspace,
    baselines: &BTreeMap<usize, Baseline>,
    held: &[String],
) -> io::Result<()> {
    let mut out = io::stdout().lock();
    let reported = located
        .iter()
        .filter(|item| is_reported(item, args, workspace, baselines));
    for item in reported {
        let recorded = baselines
            .get(&baseline_index(item, args))
            .and_then(|baseline| baseline.recorded(item));
        let message = breach_message(item, threshold_of(item, workspace), recorded);
        let line = item.finding.line;
        writeln!(
            out,
            "{}",
            github::annotation_at(Level::Error, &item.path, line, &message)
        )?;
    }
    for annotation in held {
        writeln!(out, "{annotation}")?;
    }
    Ok(())
}

/// A unit its baseline accepted at a lower score names that score, or the threshold would read
/// as if the baseline had been ignored.
fn breach_message(item: &Located, threshold: u32, recorded: Option<u32>) -> String {
    let name = item.finding.qualified_name();
    let score = item.finding.score;
    match recorded {
        Some(recorded) => format!("{name} scores {score}, above its baselined {recorded}"),
        None => format!("{name} scores {score}, over the threshold of {threshold}"),
    }
}

fn print_json(
    located: &[Located],
    args: &Args,
    workspace: &Workspace,
    baselines: &BTreeMap<usize, Baseline>,
    breaches: usize,
    gated_by: usize,
    loose_entries: Vec<ReportedLooseEntry>,
) -> io::Result<()> {
    let findings: Vec<ReportedFinding> = located
        .iter()
        .filter(|item| args.all || is_reported(item, args, workspace, baselines))
        .map(|item| ReportedFinding {
            path: item.path.display().to_string(),
            line: item.finding.line,
            end_line: item.finding.end_line,
            name: item.finding.qualified_name(),
            score: item.finding.score,
            language: item.finding.language,
            domain: workspace.domains[item.domain].name.clone(),
        })
        .collect();

    let mut thresholds = BTreeMap::new();
    for id in registry::language_ids() {
        let value = workspace
            .domains
            .get(gated_by)
            .map_or(config::DEFAULT_THRESHOLD, |domain| domain.threshold_for(id));
        thresholds.insert(id.to_string(), value);
    }

    let report = Report {
        thresholds,
        breaches,
        findings,
        loose_entries,
    };

    let json = serde_json::to_string_pretty(&report)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    writeln!(io::stdout().lock(), "{json}")
}
