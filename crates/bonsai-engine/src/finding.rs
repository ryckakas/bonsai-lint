//! Findings placed in their file and domain, and the domain-relative key a baseline records.

use std::path::PathBuf;

use bonsai_core::Finding;

/// A scored unit together with the file and domain it came from.
///
/// A finding plus where it was found. `key_path` is relative to the owning domain's root rather
/// than the working directory, so a baseline key is stable no matter where the scan is invoked
/// from and no matter which machine checks the repository out.
#[derive(Debug, Clone)]
pub struct Located {
    /// The file as reports print it, tidied by [`display_path`](crate::scan::display_path).
    pub path: PathBuf,
    /// The path a baseline records the unit under, from [`normalize_key`].
    pub key_path: String,
    /// The owning domain's index into [`Workspace::domains`](crate::Workspace::domains).
    pub domain: usize,
    /// The unit's name, line, score and suppression, as `bonsai-core` scored it.
    pub finding: Finding,
}

/// Returns `path` relative to `domain_root` with forward slashes, the form a baseline key takes.
///
/// A path outside `domain_root` is kept whole.
#[must_use]
pub fn normalize_key(path: &std::path::Path, domain_root: &std::path::Path) -> String {
    path.strip_prefix(domain_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
