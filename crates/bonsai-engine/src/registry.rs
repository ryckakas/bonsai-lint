use std::path::Path;
use std::sync::OnceLock;

use bonsai_core::LanguageDescriptor;

/// `#[cfg]` on statements keeps this compiling with any feature subset, including none. An
/// empty registry then matches no files, which the scan reports as "nothing found" rather than
/// passing silently.
pub fn descriptors() -> &'static [&'static LanguageDescriptor] {
    static ALL: OnceLock<Vec<&'static LanguageDescriptor>> = OnceLock::new();
    ALL.get_or_init(|| {
        #[allow(unused_mut)]
        let mut all: Vec<&'static LanguageDescriptor> = Vec::new();
        #[cfg(feature = "php")]
        all.push(&bonsai_lang_php::PHP);
        #[cfg(feature = "ts")]
        {
            all.push(&bonsai_lang_ts::TYPESCRIPT);
            all.push(&bonsai_lang_ts::TSX);
        }
        #[cfg(feature = "vue")]
        all.push(&bonsai_lang_vue::VUE);
        all
    })
}

/// The ids a user names in `--lang`, `--over` and a config section. Two descriptors can share
/// one spec — `.ts` and `.tsx` are one language parsed by two grammars — so this deduplicates.
#[must_use]
pub fn language_ids() -> Vec<&'static str> {
    let mut ids = Vec::new();
    for descriptor in descriptors() {
        if !ids.contains(&descriptor.spec.id) {
            ids.push(descriptor.spec.id);
        }
    }
    ids
}

/// Names that carry no code worth scoring. A declaration file holds only signatures, so every
/// unit in it scores zero; minified output rolls up into one enormous unit nobody will refactor.
/// These are whole suffixes rather than substrings, so `app.mini.js` and `min.js` are untouched.
const UNSCORED: &[&str] = &[
    ".d.ts", ".d.mts", ".d.cts", ".min.js", ".min.mjs", ".min.cjs",
];

#[must_use]
pub fn for_path(path: &Path) -> Option<&'static LanguageDescriptor> {
    let name = path.file_name()?.to_str()?;
    if UNSCORED.iter().any(|suffix| name.ends_with(suffix)) {
        return None;
    }

    let extension = path.extension()?.to_str()?;
    descriptors()
        .iter()
        .copied()
        .find(|descriptor| descriptor.extensions.contains(&extension))
}
