//! GitHub Actions workflow commands, which the runner turns into annotations on a pull request.

use std::env;
use std::fmt;
use std::path::{Path, PathBuf};

use bonsai_engine::scan::resolve;

/// The runner annotates this many errors per step and drops the rest without a word, so past it
/// the log is the only complete record.
pub(crate) const ERRORS_PER_STEP: usize = 10;

#[derive(Clone, Copy)]
pub(crate) enum Level {
    Error,
    Warning,
}

/// A command without a place lands in the run's summary; one with a file and line, on that line.
struct Annotation<'a> {
    level: Level,
    place: Option<(&'a str, usize)>,
    message: &'a str,
}

impl fmt::Display for Annotation<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let command = match self.level {
            Level::Error => "error",
            Level::Warning => "warning",
        };
        let place = self.place.map_or_else(String::new, |(file, line)| {
            format!("file={},line={line},", escape_property(file))
        });
        write!(
            f,
            "::{command} {place}title=bonsai-lint::{}",
            escape_data(self.message)
        )
    }
}

/// An annotation on the run as a whole.
pub(crate) fn annotation(level: Level, message: &str) -> String {
    Annotation {
        level,
        place: None,
        message,
    }
    .to_string()
}

/// An annotation on one line, whose message names the place too, because past
/// [`ERRORS_PER_STEP`] only the log line is left.
pub(crate) fn annotation_at(level: Level, path: &Path, line: usize, message: &str) -> String {
    let file = file(path);
    let shown = file
        .clone()
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let message = format!("{shown}:{line}: {message}");
    Annotation {
        level,
        place: file.as_deref().map(|file| (file, line)),
        message: &message,
    }
    .to_string()
}

/// An annotation on a whole file, placed on its first line: without a line the runner files it on
/// line 0, which no diff shows.
pub(crate) fn annotation_on(level: Level, path: &Path, message: &str) -> String {
    let file = file(path);
    Annotation {
        level,
        place: file.as_deref().map(|file| (file, 1)),
        message,
    }
    .to_string()
}

/// GitHub reads a relative `file` from the repository root, not from where the step ran, so under
/// `GITHUB_WORKSPACE` the path is rebased on it, and a file outside it gets no `file` at all.
fn file(path: &Path) -> Option<String> {
    let Some(workspace) = env::var_os("GITHUB_WORKSPACE").filter(|value| !value.is_empty()) else {
        return Some(path.to_string_lossy().replace('\\', "/"));
    };
    resolve(path)
        .strip_prefix(resolve(&PathBuf::from(workspace)))
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

fn escape_data(text: &str) -> String {
    text.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

fn escape_property(text: &str) -> String {
    escape_data(text).replace(':', "%3A").replace(',', "%2C")
}
