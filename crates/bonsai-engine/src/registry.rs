use std::path::Path;
use std::sync::OnceLock;

use bonsai_core::LanguageDescriptor;

/// `#[cfg]` on statements keeps this compiling with any feature subset, including none. An
/// empty registry then matches no files, which the scan reports as "nothing found" rather than
/// passing silently.
pub fn descriptors() -> &'static [&'static dyn LanguageDescriptor] {
    static ALL: OnceLock<Vec<&'static dyn LanguageDescriptor>> = OnceLock::new();
    ALL.get_or_init(|| {
        #[allow(unused_mut, reason = "with no language feature, nothing is pushed")]
        let mut all: Vec<&'static dyn LanguageDescriptor> = Vec::new();
        #[cfg(feature = "php")]
        all.push(&bonsai_lang_php::PHP);
        #[cfg(feature = "ts")]
        {
            all.push(&bonsai_lang_ts::TYPESCRIPT);
            all.push(&bonsai_lang_ts::TSX);
        }
        #[cfg(feature = "vue")]
        all.push(&bonsai_lang_vue::VUE);
        #[cfg(feature = "go")]
        all.push(&bonsai_lang_go::GO);
        #[cfg(feature = "java")]
        all.push(&bonsai_lang_java::JAVA);
        all
    })
}

/// The ids a user names in `--lang`, `--over` and a config section. Two descriptors can share
/// one spec — `.ts` and `.tsx` are one language parsed by two grammars — so this deduplicates.
#[must_use]
pub fn language_ids() -> Vec<&'static str> {
    let mut ids = Vec::new();
    for descriptor in descriptors() {
        if !ids.contains(&descriptor.spec().id) {
            ids.push(descriptor.spec().id);
        }
    }
    ids
}

#[must_use]
pub fn for_path(path: &Path) -> Option<&'static dyn LanguageDescriptor> {
    let name = path.file_name()?.to_str()?;
    let extension = path.extension()?.to_str()?;
    descriptors()
        .iter()
        .copied()
        .find(|descriptor| descriptor.extensions().contains(&extension))
        .filter(|descriptor| {
            !descriptor
                .unscored_suffixes()
                .iter()
                .any(|suffix| name.ends_with(suffix))
        })
}
