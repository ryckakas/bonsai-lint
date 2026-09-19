use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::Deserialize;

pub const CONFIG_FILE: &str = "bonsai-lint.toml";
pub const BASELINE_FILE: &str = ".bonsai-lint-baseline.json";
pub const DEFAULT_THRESHOLD: u32 = 15;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConfigFile {
    pub name: Option<String>,
    pub domains: Option<Vec<String>>,
    pub threshold: Option<u32>,
    pub exclude: Option<Vec<String>>,
    pub toplevel: Option<bool>,
    pub baseline: Option<PathBuf>,
    /// Anything left over is a per-language section. A mistyped top-level key lands here and
    /// fails to deserialise, which is preferable to being silently ignored.
    #[serde(flatten)]
    pub languages: BTreeMap<String, LanguageSection>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageSection {
    pub threshold: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct Domain {
    pub name: String,
    pub root: PathBuf,
    pub threshold: u32,
    pub language_thresholds: BTreeMap<String, u32>,
    pub toplevel: bool,
    pub baseline: PathBuf,
    /// Matched against paths relative to this domain's root; the workspace's own `exclude` is
    /// matched against the workspace root.
    pub exclude: GlobSet,
}

impl Domain {
    #[must_use]
    pub fn threshold_for(&self, language: &str) -> u32 {
        self.language_thresholds
            .get(language)
            .copied()
            .unwrap_or(self.threshold)
    }
}

/// Domains are declared by glob at the workspace root rather than discovered by walking up
/// from every file, so a stray config cannot create a domain nobody sanctioned and the whole
/// policy stays readable in one place.
#[derive(Debug)]
pub struct Workspace {
    pub root: PathBuf,
    pub domains: Vec<Domain>,
    pub exclude: GlobSet,
    pub warnings: Vec<String>,
}

impl Workspace {
    /// Longest matching root wins, so a nested domain beats the workspace root.
    #[must_use]
    pub fn domain_for(&self, path: &Path) -> usize {
        self.domains
            .iter()
            .enumerate()
            .filter(|(_, domain)| path.starts_with(&domain.root))
            .max_by_key(|(_, domain)| domain.root.as_os_str().len())
            .map_or(0, |(index, _)| index)
    }

    #[must_use]
    pub fn is_excluded(&self, path: &Path) -> bool {
        let relative = path.strip_prefix(&self.root).unwrap_or(path);
        if self.exclude.is_match(relative) {
            return true;
        }
        let domain = &self.domains[self.domain_for(path)];
        let in_domain = path.strip_prefix(&domain.root).unwrap_or(path);
        domain.exclude.is_match(in_domain)
    }

    /// A config that is not a declared domain would otherwise change nothing while looking as
    /// though it did. Only configs beneath or above a scanned path could have mattered, so a
    /// scan of one package does not walk the whole repository to find out.
    #[must_use]
    pub fn undeclared_config_warnings(&self, scanned: &[PathBuf]) -> Vec<String> {
        let mut candidates = BTreeSet::new();
        for path in scanned {
            let above = path.ancestors().take_while(|it| it.starts_with(&self.root));
            candidates.extend(above.map(Path::to_path_buf));
            candidates.extend(config_directories_below(path));
        }

        candidates
            .into_iter()
            .filter(|directory| directory.join(CONFIG_FILE).is_file())
            .filter(|directory| !self.domains.iter().any(|domain| &domain.root == directory))
            .map(|directory| {
                format!(
                    "{}: not a declared domain, ignoring (add it to `domains` in the root config)",
                    directory.join(CONFIG_FILE).display()
                )
            })
            .collect()
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Read { path: PathBuf, error: String },
    Parse { path: PathBuf, error: String },
    Glob { pattern: String, error: String },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read { path, error } | Self::Parse { path, error } => {
                write!(f, "{}: {error}", path.display())
            }
            Self::Glob { pattern, error } => write!(f, "invalid glob `{pattern}`: {error}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// No `bonsai-lint.toml` anywhere above the starting directory means zero-config: built-in
/// defaults, one implicit domain, nothing read from disk.
pub fn discover(start: &Path, known_languages: &[&str]) -> Result<Workspace, ConfigError> {
    let start = containing_directory(start);

    let Some((root, config)) = find_root(start)? else {
        // With no config anywhere, a baseline written from the project root still marks it, so
        // scanning one file afterwards finds the same accepted entries.
        let root = find_upward(start, BASELINE_FILE).unwrap_or_else(|| start.to_path_buf());
        return Ok(Workspace {
            domains: vec![default_domain(&root)],
            root,
            exclude: GlobSet::empty(),
            warnings: Vec::new(),
        });
    };

    let mut warnings = Vec::new();
    warn_unknown_languages(&config, known_languages, &root, &mut warnings);

    let exclude = build_globset(config.exclude.as_deref().unwrap_or_default())?;
    let root_domain = domain_from(
        &config,
        &root,
        &config,
        "root",
        known_languages,
        GlobSet::empty(),
    );

    let mut domains = vec![root_domain];
    if let Some(patterns) = &config.domains {
        let declared = declared_domains(&root, &config, patterns, known_languages, &mut warnings)?;
        domains.extend(declared);
    }

    Ok(Workspace {
        root,
        domains,
        exclude,
        warnings,
    })
}

fn declared_domains(
    root: &Path,
    config: &ConfigFile,
    patterns: &[String],
    known_languages: &[&str],
    warnings: &mut Vec<String>,
) -> Result<Vec<Domain>, ConfigError> {
    let matcher = build_globset(patterns)?;
    let mut domains = Vec::new();
    for directory in matching_directories(root, patterns, &matcher) {
        let path = directory.join(CONFIG_FILE);
        let child = if path.is_file() {
            read_config(&path)?
        } else {
            ConfigFile::default()
        };
        warn_unknown_languages(&child, known_languages, &directory, warnings);
        if child.domains.is_some() {
            warnings.push(format!(
                "{}: `domains` is only read from the root config, ignoring",
                path.display()
            ));
        }

        let fallback = directory
            .strip_prefix(root)
            .unwrap_or(&directory)
            .to_string_lossy()
            .to_string();
        let own_exclude = build_globset(child.exclude.as_deref().unwrap_or_default())?;
        domains.push(domain_from(
            &child,
            &directory,
            config,
            &fallback,
            known_languages,
            own_exclude,
        ));
    }
    Ok(domains)
}

fn config_directories_below(path: &Path) -> impl Iterator<Item = PathBuf> {
    ignore::WalkBuilder::new(path)
        .build()
        .flatten()
        .filter(|entry| entry.file_name() == CONFIG_FILE)
        .filter_map(|entry| entry.path().parent().map(Path::to_path_buf))
}

/// A file as the starting point must not become a domain root, or its baseline would live at
/// `file.php/.bonsai-lint-baseline.json`.
fn containing_directory(start: &Path) -> &Path {
    if start.is_dir() {
        return start;
    }
    start
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn default_domain(root: &Path) -> Domain {
    Domain {
        name: "root".to_string(),
        root: root.to_path_buf(),
        threshold: DEFAULT_THRESHOLD,
        language_thresholds: BTreeMap::new(),
        toplevel: true,
        baseline: root.join(BASELINE_FILE),
        exclude: GlobSet::empty(),
    }
}

/// Precedence runs nearest-and-most-specific first: the domain's own language section, then
/// its general threshold, then the root's language section, then the root's general threshold.
/// A nearer domain therefore wins even when the root set something more specific, which is what
/// makes `threshold = 5` in a domain mean what it says.
fn domain_from(
    config: &ConfigFile,
    root: &Path,
    inherited: &ConfigFile,
    fallback: &str,
    known_languages: &[&str],
    exclude: GlobSet,
) -> Domain {
    let threshold = config
        .threshold
        .or(inherited.threshold)
        .unwrap_or(DEFAULT_THRESHOLD);

    let mut language_thresholds: BTreeMap<String, u32> = BTreeMap::new();
    for language in known_languages {
        let value = config
            .languages
            .get(*language)
            .and_then(|section| section.threshold)
            .or(config.threshold)
            .or_else(|| {
                inherited
                    .languages
                    .get(*language)
                    .and_then(|section| section.threshold)
            })
            .or(inherited.threshold)
            .unwrap_or(DEFAULT_THRESHOLD);
        language_thresholds.insert((*language).to_string(), value);
    }

    Domain {
        name: config.name.clone().unwrap_or_else(|| fallback.to_string()),
        root: root.to_path_buf(),
        threshold,
        language_thresholds,
        toplevel: config.toplevel.or(inherited.toplevel).unwrap_or(true),
        baseline: config
            .baseline
            .clone()
            .map_or_else(|| root.join(BASELINE_FILE), |path| root.join(path)),
        exclude,
    }
}

/// The outermost config that declares `domains` is the root. A domain's own config, or one
/// nobody declared, must not re-root the workspace just because the scan started inside it, or
/// `bonsai-lint packages/web` would answer differently from `bonsai-lint .`.
fn find_root(start: &Path) -> Result<Option<(PathBuf, ConfigFile)>, ConfigError> {
    let mut root = None;
    for directory in start.ancestors() {
        let path = directory.join(CONFIG_FILE);
        if directory.as_os_str().is_empty() || !path.is_file() {
            continue;
        }
        let config = read_config(&path)?;
        if root.is_none() || config.domains.is_some() {
            root = Some((directory.to_path_buf(), config));
        }
    }
    Ok(root)
}

fn find_upward(start: &Path, file_name: &str) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(directory) = current {
        if directory.join(file_name).is_file() {
            return Some(directory.to_path_buf());
        }
        current = directory.parent();
    }
    None
}

fn read_config(path: &Path) -> Result<ConfigFile, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|error| ConfigError::Read {
        path: path.to_path_buf(),
        error: error.to_string(),
    })?;
    toml::from_str(&text).map_err(|error| ConfigError::Parse {
        path: path.to_path_buf(),
        error: error.to_string(),
    })
}

/// `*` stops at `/`, as in `.gitignore`, so `packages/*` names direct children only and
/// `vendor/**` is how a subtree is spelled.
fn build_globset(patterns: &[String]) -> Result<GlobSet, ConfigError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        if pattern.starts_with('!') {
            return Err(ConfigError::Glob {
                pattern: pattern.clone(),
                error: "negated patterns are not supported".to_string(),
            });
        }
        let glob = GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map_err(|error| ConfigError::Glob {
                pattern: pattern.clone(),
                error: error.to_string(),
            })?;
        builder.add(glob);
    }
    builder.build().map_err(|error| ConfigError::Glob {
        pattern: patterns.join(", "),
        error: error.to_string(),
    })
}

fn matching_directories(root: &Path, patterns: &[String], matcher: &GlobSet) -> Vec<PathBuf> {
    let mut walker = ignore::WalkBuilder::new(root);
    walker.max_depth(pattern_depth(patterns));

    let mut found = Vec::new();
    for entry in walker.build().flatten() {
        if !entry.file_type().is_some_and(|kind| kind.is_dir()) {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        if !relative.as_os_str().is_empty() && matcher.is_match(relative) {
            found.push(entry.path().to_path_buf());
        }
    }
    found.sort();
    found
}

/// `packages/*` can only match two levels down, so the walk stops there instead of visiting the
/// whole repository on every editor keystroke.
fn pattern_depth(patterns: &[String]) -> Option<usize> {
    patterns.iter().try_fold(0, |deepest, pattern| {
        if pattern.contains("**") {
            return None;
        }
        Some(deepest.max(pattern.trim_matches('/').split('/').count()))
    })
}

fn warn_unknown_languages(
    config: &ConfigFile,
    known: &[&str],
    at: &Path,
    warnings: &mut Vec<String>,
) {
    for language in config.languages.keys() {
        if !known.contains(&language.as_str()) {
            warnings.push(format!(
                "{}: unknown language section `[{language}]`, ignoring",
                at.join(CONFIG_FILE).display()
            ));
        }
    }
}
