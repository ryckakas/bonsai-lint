use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bonsai_core::Suppression;
use bonsai_engine::config::{self, Workspace};
use bonsai_engine::finding::normalize_key;
use bonsai_engine::report::{Report, ReportedFinding};
use bonsai_engine::scan::{decode, display_path, rank, resolve};
use bonsai_engine::{registry, Baseline, Located, ScanOutcome, ScanStats, Scanner, STACK_SIZE};
use clap::{Parser as ClapParser, ValueEnum};

#[derive(ClapParser)]
#[command(
    name = "bonsai-lint",
    version,
    about = "Cognitive complexity linter for PHP, JS, TS, Vue and Go"
)]
#[allow(clippy::struct_excessive_bools)]
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

    /// Output format; `json` is the machine-readable form editors consume
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,

    /// Record current findings as accepted, so only later regressions fail
    #[arg(long)]
    write_baseline: bool,

    /// Use one baseline file, keyed from the workspace root, instead of each domain's own
    #[arg(long, value_name = "PATH")]
    baseline: Option<PathBuf>,

    /// Restrict the scan to these languages (`php`, `typescript`, `vue`, `go`)
    #[arg(long, value_delimiter = ',', value_name = "ID")]
    lang: Vec<String>,

    /// Restrict the scan to one named domain
    #[arg(long, value_name = "NAME")]
    domain: Option<String>,

    /// Start config discovery here instead of the first scanned path
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

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
}

fn main() -> ExitCode {
    let args = Args::parse();

    // The engine gives its own workers this stack; `--stdin` parses on this thread instead.
    let outcome = std::thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(move || run(&args))
        .map_err(|error| format!("could not start the scan: {error}"))
        .and_then(|worker| worker.join().map_err(|_| "the scan aborted".to_string()));

    match outcome {
        Ok(Ok(code)) => code,
        Ok(Err(message)) | Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
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
        eprintln!("{warning}");
    }
    if !args.stdin {
        for warning in workspace.undeclared_config_warnings(&scan_roots) {
            eprintln!("{warning}");
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

    for message in warnings.iter().chain(&errors) {
        eprintln!("{message}");
    }

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

    warn_about_unreasoned_suppressions(&located, &workspace);

    let over_threshold: Vec<Located> = located
        .iter()
        .filter(|item| is_over_threshold(item, &workspace))
        .cloned()
        .collect();

    if args.write_baseline {
        return write_baselines(&over_threshold, args, &workspace);
    }

    let baselines = load_baselines(args, &workspace)?;
    report_stale_entries(&baselines, &located, &workspace, args, &scan_roots);

    let breaches: Vec<&Located> = over_threshold
        .iter()
        .filter(|item| is_regression(item, &baselines, args))
        .collect();

    let gated_by = domain_filter.unwrap_or_else(|| only_visited_domain(&stats));
    let printed = match args.format {
        Format::Text => print_text(&located, args, &workspace, &baselines),
        Format::Json => print_json(
            &located,
            args,
            &workspace,
            &baselines,
            breaches.len(),
            gated_by,
        ),
    };
    tolerate_closed_pipe(printed)?;

    if breaches.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }

    if args.format == Format::Text {
        let accepted = over_threshold.len() - breaches.len();
        eprintln!(
            "\n{} unit(s) over the threshold{}",
            breaches.len(),
            if accepted > 0 {
                format!(", {accepted} accepted by a baseline")
            } else {
                String::new()
            }
        );
    }

    Ok(ExitCode::FAILURE)
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

    let source = decode(bytes, &path, &mut outcome.warnings);
    if descriptor.generated(&source) {
        return Ok(outcome);
    }
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

fn is_over_threshold(item: &Located, workspace: &Workspace) -> bool {
    let threshold = workspace.domains[item.domain].threshold_for(item.finding.language);
    item.finding.score > threshold && !item.finding.is_suppressed()
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
fn report_stale_entries(
    baselines: &BTreeMap<usize, Baseline>,
    located: &[Located],
    workspace: &Workspace,
    args: &Args,
    scan_roots: &[PathBuf],
) {
    if args.stdin || args.domain.is_some() || !args.lang.is_empty() {
        return;
    }
    let covered = |root: &Path| scan_roots.iter().any(|path| root.starts_with(path));

    for (index, baseline) in baselines {
        let (name, root, scoped) = if args.baseline.is_some() {
            ("baseline", workspace.root.as_path(), located.to_vec())
        } else {
            let domain = &workspace.domains[*index];
            let scoped = located
                .iter()
                .filter(|item| item.domain == *index)
                .cloned()
                .collect();
            (domain.name.as_str(), domain.root.as_path(), scoped)
        };

        if !covered(root) {
            continue;
        }
        let stale = baseline.unmatched(&scoped);
        if !stale.is_empty() {
            eprintln!(
                "{name}: {} baseline entr(ies) matched nothing in this scan",
                stale.len()
            );
        }
    }
}

/// A marker without a reason is refused rather than obeyed, so it has to say so out loud —
/// otherwise the author would believe the finding was silenced.
fn warn_about_unreasoned_suppressions(located: &[Located], workspace: &Workspace) {
    for item in located {
        let threshold = workspace.domains[item.domain].threshold_for(item.finding.language);
        if item.finding.score > threshold && item.finding.suppression == Suppression::MissingReason
        {
            eprintln!(
                "{}:{}: `{}` needs a reason, e.g. `// {}: why this has to stay complex` — \
                 ignoring it and reporting {}",
                item.path.display(),
                item.finding.line,
                bonsai_core::SUPPRESSION_MARKER,
                bonsai_core::SUPPRESSION_MARKER,
                item.finding.qualified_name()
            );
        }
    }
}

fn unusable_scan(stats: &ScanStats, args: &Args) -> Option<String> {
    if stats.errors > 0 {
        return Some(format!(
            "{} path(s) could not be read; refusing to report a clean run",
            stats.errors
        ));
    }

    if stats.files == 0 {
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

fn print_json(
    located: &[Located],
    args: &Args,
    workspace: &Workspace,
    baselines: &BTreeMap<usize, Baseline>,
    breaches: usize,
    gated_by: usize,
) -> io::Result<()> {
    let findings: Vec<ReportedFinding> = located
        .iter()
        .filter(|item| args.all || is_reported(item, args, workspace, baselines))
        .map(|item| ReportedFinding {
            path: item.path.display().to_string(),
            line: item.finding.line,
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
    };

    let json = serde_json::to_string_pretty(&report)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    writeln!(io::stdout().lock(), "{json}")
}
