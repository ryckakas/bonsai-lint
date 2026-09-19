use std::path::PathBuf;

use bonsai_core::Finding;

/// A finding plus where it was found. `key_path` is relative to the owning domain's root rather
/// than the working directory, so a baseline key is stable no matter where the scan is invoked
/// from and no matter which machine checks the repository out.
#[derive(Debug, Clone)]
pub struct Located {
    pub path: PathBuf,
    pub key_path: String,
    pub domain: usize,
    pub finding: Finding,
}

#[must_use]
pub fn normalize_key(path: &std::path::Path, domain_root: &std::path::Path) -> String {
    path.strip_prefix(domain_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
