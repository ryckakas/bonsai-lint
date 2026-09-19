use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
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
        self.exclude.is_match(relative)
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

/// No `bonsai-lint.toml` anywhere above the starting directory means zero-config: built-in defaults,
/// one implicit domain, nothing read from disk.
pub fn discover(start: &Path, known_languages: &[&str]) -> Result<Workspace, ConfigError> {
    let Some(root) = find_workspace_root(start) else {
        return Ok(Workspace {
            root: start.to_path_buf(),
            domains: vec![default_domain(start)],
            exclude: GlobSet::empty(),
            warnings: Vec::new(),
        });
    };

    let mut warnings = Vec::new();
    let config = read_config(&root.join(CONFIG_FILE))?;
    warn_unknown_languages(&config, known_languages, &root, &mut warnings);

    let exclude = build_globset(config.exclude.as_deref().unwrap_or_default())?;
    let root_domain = domain_from(&config, &root, &config, "root", known_languages);

    let mut domains = vec![root_domain];

    if let Some(patterns) = &config.domains {
        let matcher = build_globset(patterns)?;
        for directory in matching_directories(&root, &matcher) {
            let path = directory.join(CONFIG_FILE);
            let child = if path.is_file() {
                read_config(&path)?
            } else {
                ConfigFile::default()
            };
            warn_unknown_languages(&child, known_languages, &directory, &mut warnings);

            let fallback = directory
                .strip_prefix(&root)
                .unwrap_or(&directory)
                .to_string_lossy()
                .to_string();
            domains.push(domain_from(
                &child,
                &directory,
                &config,
                &fallback,
                known_languages,
            ));
        }
    }

    warn_undeclared_configs(&root, &domains, &mut warnings);

    Ok(Workspace {
        root,
        domains,
        exclude,
        warnings,
    })
}

fn default_domain(root: &Path) -> Domain {
    Domain {
        name: "root".to_string(),
        root: root.to_path_buf(),
        threshold: DEFAULT_THRESHOLD,
        language_thresholds: BTreeMap::new(),
        toplevel: true,
        baseline: root.join(BASELINE_FILE),
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
    }
}

fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_dir() {
        Some(start)
    } else {
        start.parent()
    };
    while let Some(directory) = current {
        if directory.join(CONFIG_FILE).is_file() {
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

fn build_globset(patterns: &[String]) -> Result<GlobSet, ConfigError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        if pattern.starts_with('!') {
            continue;
        }
        let glob = Glob::new(pattern).map_err(|error| ConfigError::Glob {
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

fn matching_directories(root: &Path, matcher: &GlobSet) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in ignore::WalkBuilder::new(root).build().flatten() {
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

/// A config that is not a declared domain would otherwise change nothing while looking as
/// though it did, so it is named out loud rather than passed over.
fn warn_undeclared_configs(root: &Path, domains: &[Domain], warnings: &mut Vec<String>) {
    for entry in ignore::WalkBuilder::new(root).build().flatten() {
        if entry.file_name() != CONFIG_FILE {
            continue;
        }
        let Some(directory) = entry.path().parent() else {
            continue;
        };
        if !domains.iter().any(|domain| domain.root == directory) {
            warnings.push(format!(
                "{}: not a declared domain, ignoring (add it to `domains` in the root config)",
                entry.path().display()
            ));
        }
    }
}
