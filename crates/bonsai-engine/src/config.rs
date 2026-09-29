//! Configuration discovery, which reads `bonsai-lint.toml` files into a [`Workspace`] of domains.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::Deserialize;

use crate::finding::normalize_key;

/// The configuration file's name, read at the workspace root and in each declared domain.
pub const CONFIG_FILE: &str = "bonsai-lint.toml";
/// The baseline's file name in a domain's root, unless its config sets `baseline`.
///
/// With no config above the scan, the nearest directory holding this file is the workspace root.
pub const BASELINE_FILE: &str = ".bonsai-lint-baseline.json";
/// The highest score that passes wherever no config sets another.
pub const DEFAULT_THRESHOLD: u32 = 15;

/// Every other top-level key must be a `[language]` table.
const KEYS: &[&str] = &[
    "name",
    "domains",
    "threshold",
    "exclude",
    "toplevel",
    "baseline",
    "strict-baseline",
];

/// One `bonsai-lint.toml` as written, before inheritance and defaults fill in what it leaves out.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConfigFile {
    /// The name reports and `--domain` know the domain by.
    ///
    /// `None` means `root` for the workspace root, and a domain's path from that root otherwise.
    pub name: Option<String>,
    /// Globs naming the directories that are domains, read only from the root config.
    pub domains: Option<Vec<String>>,
    /// The highest score that passes, unless a language section in this file sets its own.
    pub threshold: Option<u32>,
    /// Globs of paths to skip, relative to this config's directory.
    pub exclude: Option<Vec<String>>,
    /// Whether code outside any function is scored as `<toplevel>`.
    ///
    /// `None` in a domain takes the root's setting, and `None` at the root means `true`.
    pub toplevel: Option<bool>,
    /// Where this directory's baseline lives, relative to it.
    pub baseline: Option<PathBuf>,
    /// Whether a baseline entry looser than the code fails the run, written `strict-baseline`.
    ///
    /// `None` in a domain takes the root's setting, and `None` at the root means `false`.
    #[serde(rename = "strict-baseline")]
    pub strict_baseline: Option<bool>,
    /// Anything left over is a per-language section. A mistyped top-level key lands here and
    /// fails to deserialise, which is preferable to being silently ignored.
    #[serde(flatten)]
    pub languages: BTreeMap<String, LanguageSection>,
}

/// A `[language]` table, named by a language id.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageSection {
    /// The highest score that passes in that language, overriding the file's own `threshold`.
    pub threshold: Option<u32>,
}

/// A directory with its own thresholds, excludes and baseline, the root config's settings
/// already folded in.
///
/// The workspace root is always a domain, the first in [`Workspace::domains`].
#[derive(Debug, Clone)]
pub struct Domain {
    /// The name reports and `--domain` use, unique within the workspace.
    pub name: String,
    /// The directory the domain covers, and the base of its baseline keys and `exclude` globs.
    pub root: PathBuf,
    /// The highest score that passes in a language missing from `language_thresholds`.
    pub threshold: u32,
    /// The highest score that passes, per language id.
    pub language_thresholds: BTreeMap<String, u32>,
    /// Whether code outside any function is scored as `<toplevel>`.
    pub toplevel: bool,
    /// Where the domain's baseline file lives.
    pub baseline: PathBuf,
    /// Whether an entry in the domain's baseline that is looser than the code fails the run.
    pub strict_baseline: bool,
    /// Matched against paths relative to this domain's root; the workspace's own `exclude` is
    /// matched against the workspace root.
    pub exclude: GlobSet,
}

impl Domain {
    /// Returns the highest score that passes for the language id `language`.
    #[must_use]
    pub fn threshold_for(&self, language: &str) -> u32 {
        self.language_thresholds
            .get(language)
            .copied()
            .unwrap_or(self.threshold)
    }
}

/// A configured project: its root, its domains, and the root's excludes.
///
/// Domains are declared by glob at the workspace root rather than discovered by walking up
/// from every file, so a stray config cannot create a domain nobody sanctioned and the whole
/// policy stays readable in one place.
#[derive(Debug)]
pub struct Workspace {
    /// The directory holding the root config.
    ///
    /// Without any config, it is the nearest directory at or above the one [`discover`] started
    /// from that holds a [`BASELINE_FILE`], or else that starting directory.
    pub root: PathBuf,
    /// The root domain first, then every declared domain in path order.
    pub domains: Vec<Domain>,
    /// The root config's `exclude` globs, matched against paths relative to the workspace root.
    pub exclude: GlobSet,
    /// Config problems that were ignored rather than refused, such as an unknown language section.
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

    /// Returns whether `path` matches the root's `exclude` or its own domain's.
    ///
    /// Like the domain roots, `path` is expected absolute and canonical, as
    /// [`resolve`](crate::scan::resolve) makes it.
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

/// Why the configuration could not be read into a [`Workspace`].
#[derive(Debug)]
pub enum ConfigError {
    /// A config file could not be read from disk.
    Read {
        /// The config file.
        path: PathBuf,
        /// What went wrong, as a message for the user.
        error: String,
    },
    /// A config file is not valid TOML or does not fit the config's shape, such as an unknown key.
    Parse {
        /// The config file.
        path: PathBuf,
        /// What went wrong, as a message for the user.
        error: String,
    },
    /// A config file parsed but clashes with the rest of the workspace, such as a repeated name.
    Invalid {
        /// The config file of the domain at fault, which a domain declared only by glob lacks.
        path: PathBuf,
        /// What went wrong, as a message for the user.
        error: String,
    },
    /// An `exclude` or `domains` glob that does not compile, or is negated.
    Glob {
        /// The pattern at fault, or every pattern joined by commas when the set failed to build.
        pattern: String,
        /// What went wrong, as a message for the user.
        error: String,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read { path, error }
            | Self::Parse { path, error }
            | Self::Invalid { path, error } => {
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
    reject_shared_names(&root, &domains)?;

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

        // A domain's name reaches the user through reports and `--domain`, so it is spelled
        // with forward slashes on every platform rather than the host separator.
        let fallback = normalize_key(&directory, root);
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

/// `--domain` and the report's `domain` field know a domain by its name alone, so two domains
/// sharing one could not be told apart.
fn reject_shared_names(root: &Path, domains: &[Domain]) -> Result<(), ConfigError> {
    let clash = domains.iter().enumerate().find_map(|(index, domain)| {
        let first = domains[..index]
            .iter()
            .find(|other| other.name == domain.name)?;
        Some((first, domain))
    });
    let Some((first, domain)) = clash else {
        return Ok(());
    };
    let owner = if first.root == root {
        "the workspace root".to_string()
    } else {
        format!("`{}`", normalize_key(&first.root, root))
    };
    Err(ConfigError::Invalid {
        path: domain.root.join(CONFIG_FILE),
        error: format!(
            "the domain name `{}` is also the name of {owner}",
            domain.name
        ),
    })
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
        strict_baseline: false,
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
        strict_baseline: config
            .strict_baseline
            .or(inherited.strict_baseline)
            .unwrap_or(false),
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
    let parse_error = |error: String| ConfigError::Parse {
        path: path.to_path_buf(),
        error,
    };
    // Any other key is read as a language section, so a misspelt one would be reported as a
    // section of the wrong shape rather than by its name.
    let table: toml::Table =
        toml::from_str(&text).map_err(|error| parse_error(error.to_string()))?;
    let unknown = table
        .iter()
        .find(|(key, value)| !KEYS.contains(&key.as_str()) && !value.is_table());
    if let Some((key, _)) = unknown {
        let hint = suggestion(key, KEYS, |near| format!("`{near}`"));
        return Err(parse_error(format!("unknown key `{key}`{hint}")));
    }
    toml::from_str(&text).map_err(|error| parse_error(error.to_string()))
}

/// `; did you mean `threshold`?` for a near miss, and nothing for a word close to none of them.
fn suggestion(word: &str, candidates: &[&str], quote: impl Fn(&str) -> String) -> String {
    candidates
        .iter()
        .map(|candidate| (edit_distance(word, candidate), candidate))
        .filter(|(distance, _)| *distance <= 2)
        .min_by_key(|(distance, _)| *distance)
        .map_or_else(String::new, |(_, near)| {
            format!("; did you mean {}?", quote(near))
        })
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, left) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, right) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(left != *right))
                .min(row[j] + 1)
                .min(above + 1);
            diagonal = above;
        }
    }
    row[b.len()]
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
            let hint = suggestion(language, known, |near| format!("`[{near}]`"));
            warnings.push(format!(
                "{}: unknown language section `[{language}]`, ignoring{hint}",
                at.join(CONFIG_FILE).display()
            ));
        }
    }
}
