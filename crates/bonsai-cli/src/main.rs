use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use bonsai_core::Suppression;
use bonsai_lint::config::{self, Workspace};
use bonsai_lint::report::{Report, ReportedFinding};
use bonsai_lint::{registry, Baseline, Located, Scanner};
use clap::{Parser as ClapParser, ValueEnum};

#[derive(ClapParser)]
#[command(
    name = "bonsai",
    version,
    about = "Cognitive complexity linter for PHP, JS and TS"
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

    /// Use one baseline file instead of each domain's own
    #[arg(long, value_name = "PATH")]
    baseline: Option<PathBuf>,

    /// Restrict the scan to these languages
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
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum Format {
    Text,
    Json,
}

fn main() -> ExitCode {
    let args = Args::parse();

    match run(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<ExitCode, String> {
    let known: Vec<&str> = registry::descriptors().iter().map(|d| d.id).collect();
    let start = args
        .config
        .clone()
        .or_else(|| args.paths.first().cloned())
        .unwrap_or_else(|| PathBuf::from("."));
    let start = start.canonicalize().unwrap_or(start);

    let mut workspace = config::discover(&start, &known).map_err(|error| error.to_string())?;
    apply_overrides(args, &mut workspace)?;

    for warning in &workspace.warnings {
        eprintln!("{warning}");
    }

    let mut scanner = Scanner::new();
    let languages = (!args.lang.is_empty()).then(|| args.lang.clone());

    let (mut located, stats, errors) = if args.stdin {
        scan_stdin(args, &workspace, &mut scanner)?
    } else {
        let (located, stats, errors) = scanner.scan(&args.paths, &workspace, languages.as_deref());
        (located, stats, errors)
    };

    for error in &errors {
        eprintln!("{error}");
    }

    if let Some(wanted) = &args.domain {
        let index = workspace
            .domains
            .iter()
            .position(|domain| &domain.name == wanted)
            .ok_or_else(|| format!("no domain named `{wanted}`"))?;
        located.retain(|item| item.domain == index);
    }

    // A gate that could not read what it was pointed at must not report success; otherwise a
    // renamed directory turns the check into a no-op that stays green.
    if let Some(failure) = unusable_scan(&stats, args) {
        return Err(failure);
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
    report_stale_entries(&baselines, &located, &workspace, &stats);

    let breaches: Vec<&Located> = over_threshold
        .iter()
        .filter(|item| is_regression(item, &baselines, args))
        .collect();

    match args.format {
        Format::Text => print_text(&located, args, &workspace, &baselines),
        Format::Json => print_json(&located, args, &workspace, &baselines, breaches.len()),
    }

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

type ScanResult = (Vec<Located>, bonsai_lint::ScanStats, Vec<String>);

fn scan_stdin(
    args: &Args,
    workspace: &Workspace,
    scanner: &mut Scanner,
) -> Result<ScanResult, String> {
    let path = args
        .stdin_path
        .clone()
        .ok_or_else(|| "--stdin needs --stdin-path to know which language to use".to_string())?;

    let descriptor = registry::for_path(&path)
        .ok_or_else(|| format!("{}: no language handles this extension", path.display()))?;

    let mut source = String::new();
    std::io::stdin()
        .read_to_string(&mut source)
        .map_err(|error| format!("reading stdin: {error}"))?;

    let absolute = path.canonicalize().unwrap_or_else(|_| path.clone());
    let index = workspace.domain_for(&absolute);
    let domain = &workspace.domains[index];
    let key_path = bonsai_lint::finding::normalize_key(&absolute, &domain.root);

    let located = scanner
        .analyze_source(descriptor, &source, domain.toplevel)
        .into_iter()
        .map(|finding| Located {
            path: path.clone(),
            key_path: key_path.clone(),
            domain: index,
            finding,
        })
        .collect();

    let stats = bonsai_lint::ScanStats {
        files: 1,
        errors: 0,
        domains: std::iter::once(index).collect(),
    };

    Ok((located, stats, Vec::new()))
}

fn apply_overrides(args: &Args, workspace: &mut Workspace) -> Result<(), String> {
    let mut global: Option<u32> = None;
    let mut per_language: BTreeMap<String, u32> = BTreeMap::new();

    for value in &args.over {
        for part in value.split(',') {
            match part.split_once('=') {
                Some((language, number)) => {
                    let parsed = number
                        .trim()
                        .parse::<u32>()
                        .map_err(|_| format!("--over: `{number}` is not a number"))?;
                    per_language.insert(language.trim().to_string(), parsed);
                }
                None => {
                    global = Some(
                        part.trim()
                            .parse::<u32>()
                            .map_err(|_| format!("--over: `{part}` is not a number"))?,
                    );
                }
            }
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

fn is_over_threshold(item: &Located, workspace: &Workspace) -> bool {
    let threshold = workspace.domains[item.domain].threshold_for(item.finding.language);
    item.finding.score > threshold && !item.finding.is_suppressed()
}

fn is_regression(item: &Located, baselines: &BTreeMap<usize, Baseline>, args: &Args) -> bool {
    let key = if args.baseline.is_some() {
        0
    } else {
        item.domain
    };
    baselines
        .get(&key)
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
        println!(
            "recorded {} finding(s) in {}",
            baseline.len(),
            path.display()
        );
        return Ok(ExitCode::SUCCESS);
    }

    let mut by_domain: BTreeMap<usize, Vec<Located>> = BTreeMap::new();
    for item in over_threshold {
        by_domain.entry(item.domain).or_default().push(item.clone());
    }

    for (index, domain) in workspace.domains.iter().enumerate() {
        let items = by_domain.remove(&index).unwrap_or_default();
        if items.is_empty() && !domain.baseline.exists() {
            continue;
        }
        let baseline = Baseline::from_findings(&items);
        baseline
            .save(&domain.baseline)
            .map_err(|error| format!("{}: {error}", domain.baseline.display()))?;
        println!(
            "recorded {} finding(s) in {}",
            baseline.len(),
            domain.baseline.display()
        );
    }

    Ok(ExitCode::SUCCESS)
}

/// Stale keys are how a baseline rots into a permanent amnesty.
fn report_stale_entries(
    baselines: &BTreeMap<usize, Baseline>,
    located: &[Located],
    workspace: &Workspace,
    stats: &bonsai_lint::ScanStats,
) {
    for (index, baseline) in baselines {
        if !stats.domains.contains(index) {
            continue;
        }
        let scoped: Vec<Located> = located
            .iter()
            .filter(|item| item.domain == *index)
            .cloned()
            .collect();
        let stale = baseline.unmatched(&scoped);
        if !stale.is_empty() {
            let name = workspace
                .domains
                .get(*index)
                .map_or("root", |domain| &domain.name);
            eprintln!(
                "{}: {} baseline entr(ies) matched nothing in this scan",
                name,
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

fn unusable_scan(stats: &bonsai_lint::ScanStats, args: &Args) -> Option<String> {
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

fn print_text(
    located: &[Located],
    args: &Args,
    workspace: &Workspace,
    baselines: &BTreeMap<usize, Baseline>,
) {
    for item in located {
        if args.all || is_reported(item, args, workspace, baselines) {
            println!(
                "{:>4}  {}:{}  {}",
                item.finding.score,
                item.path.display(),
                item.finding.line,
                item.finding.qualified_name()
            );
        }
    }
}

fn print_json(
    located: &[Located],
    args: &Args,
    workspace: &Workspace,
    baselines: &BTreeMap<usize, Baseline>,
    breaches: usize,
) {
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
    for descriptor in registry::descriptors() {
        let value = workspace
            .domains
            .first()
            .map_or(config::DEFAULT_THRESHOLD, |domain| {
                domain.threshold_for(descriptor.id)
            });
        thresholds.insert(descriptor.id.to_string(), value);
    }

    let report = Report {
        thresholds,
        breaches,
        findings,
    };

    match serde_json::to_string_pretty(&report) {
        Ok(json) => println!("{json}"),
        Err(error) => eprintln!("failed to serialise the report: {error}"),
    }
}
