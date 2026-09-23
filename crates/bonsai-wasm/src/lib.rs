//! WebAssembly bindings for the scorer, so a browser can score pasted source with no server.
//!
//! This deliberately does not depend on `bonsai-engine`. The engine's job is walking a
//! filesystem, scheduling work across threads and reconciling baselines; none of that exists
//! in a browser with one pasted buffer. What remains is the ten-line driver loop below,
//! reading the same `LanguageDescriptor`s and calling the same `bonsai_core::analyze`, so a
//! score here is the score the CLI would print.
//!
//! The ABI is raw C rather than wasm-bindgen: the whole surface is one string in and one
//! string out, which does not justify a code-generation toolchain in the build.

use std::alloc::{alloc, dealloc, Layout};
use std::cell::RefCell;
use std::fmt::Write as _;

use bonsai_core::{Finding, LanguageDescriptor};

thread_local! {
    /// Analysis writes here and `result_ptr`/`result_len` hand the span to the host, which
    /// avoids making the caller guess an output capacity up front.
    static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Language selector shared with the JavaScript wrapper. Kept as integers because the ABI
/// carries no strings in that direction.
const LANG_TYPESCRIPT: u32 = 0;
const LANG_PHP: u32 = 1;
const LANG_VUE: u32 = 2;
const LANG_GO: u32 = 3;

// A raw `Layout` rather than `Vec::with_capacity`, because freeing has to name the exact
// layout that was allocated and `with_capacity` only promises *at least* the requested size.
fn layout(len: usize) -> Option<Layout> {
    Layout::from_size_align(len, 1).ok()
}

/// Reserves `len` bytes for the host to write source into, or returns null if that is refused.
/// The host owns the allocation until it calls [`bl_free`] with the same `len`.
#[no_mangle]
pub extern "C" fn bl_alloc(len: usize) -> *mut u8 {
    match layout(len) {
        Some(layout) if len > 0 => unsafe { alloc(layout) },
        _ => std::ptr::null_mut(),
    }
}

/// Releases an allocation made by [`bl_alloc`].
///
/// # Safety
/// `ptr` must come from [`bl_alloc`] with the same `len`, and must not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn bl_free(ptr: *mut u8, len: usize) {
    if let Some(layout) = layout(len) {
        if !ptr.is_null() && len > 0 {
            unsafe { dealloc(ptr, layout) };
        }
    }
}

/// Scores `len` bytes of UTF-8 source at `ptr` and returns the length of the JSON result,
/// which the host then reads from [`bl_result_ptr`].
///
/// Returns 0 for invalid UTF-8, an unknown language, or a grammar that refuses the source.
///
/// # Safety
/// `ptr` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn bl_analyze(ptr: *const u8, len: usize, language: u32) -> usize {
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let Ok(source) = std::str::from_utf8(bytes) else {
        return 0;
    };

    let json = match analyze(source, language) {
        Some(findings) => to_json(&findings),
        None => return 0,
    };

    RESULT.with(|cell| {
        let mut out = cell.borrow_mut();
        out.clear();
        out.extend_from_slice(json.as_bytes());
        out.len()
    })
}

#[no_mangle]
pub extern "C" fn bl_result_ptr() -> *const u8 {
    RESULT.with(|cell| cell.borrow().as_ptr())
}

fn descriptor(language: u32) -> Option<&'static LanguageDescriptor> {
    match language {
        LANG_TYPESCRIPT => Some(&bonsai_lang_ts::TYPESCRIPT),
        LANG_PHP => Some(&bonsai_lang_php::PHP),
        LANG_VUE => Some(&bonsai_lang_vue::VUE),
        LANG_GO => Some(&bonsai_lang_go::GO),
        _ => None,
    }
}

/// The browser-side equivalent of the engine's per-file path: pick a grammar, parse, score.
/// `toplevel` is always on, because a pasted snippet is usually not inside a function and
/// reporting nothing for it would be baffling.
fn analyze(source: &str, language: u32) -> Option<Vec<Finding>> {
    let descriptor = descriptor(language)?;
    if descriptor.generated(source) {
        return Some(Vec::new());
    }

    // A language embedded in a host syntax — Vue — resolves its own grammar and the ranges
    // worth parsing. Everything else parses the whole buffer with one grammar.
    let (compiled, ranges) = match descriptor.extract {
        Some(extract) => {
            let extraction = extract(source);
            (extraction.language, extraction.ranges)
        }
        None => ((descriptor.compiled)(), Vec::new()),
    };

    // No script blocks found. Parsing the whole document with the script grammar would score
    // the template as if it were code, so report nothing instead.
    if descriptor.extract.is_some() && ranges.is_empty() {
        return Some(Vec::new());
    }

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&compiled.ts).ok()?;
    if !ranges.is_empty() {
        parser.set_included_ranges(&ranges).ok()?;
    }
    let tree = parser.parse(source, None)?;

    Some(bonsai_core::analyze(
        &tree,
        source.as_bytes(),
        compiled,
        true,
    ))
}

/// Hand-rolled rather than via serde: the shape is three fields and pulling in a serialiser
/// costs more wasm than the whole scorer saves.
fn to_json(findings: &[Finding]) -> String {
    let mut out = String::from("[");
    for (i, finding) in findings.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"score\":");
        out.push_str(&finding.score.to_string());
        out.push_str(",\"line\":");
        out.push_str(&finding.line.to_string());
        out.push_str(",\"name\":\"");
        escape_into(&finding.qualified_name(), &mut out);
        out.push_str("\",\"suppressed\":");
        out.push_str(if finding.is_suppressed() {
            "true"
        } else {
            "false"
        });
        out.push('}');
    }
    out.push(']');
    out
}

fn escape_into(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                // Writing into a String is infallible, so there is no error case to handle.
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
}
