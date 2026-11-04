use crate::languages::{resolve_relative_or_exact, strip_import_quotes};
use std::collections::HashMap;

pub(crate) fn resolve_c_family_include(
    importer_path: &str,
    raw_target: &str,
    file_index: &HashMap<&str, u32>,
) -> Option<u32> {
    let trimmed = raw_target.trim();
    if trimmed.starts_with('<') {
        return None;
    }
    let clean = strip_import_quotes(trimmed);
    resolve_relative_or_exact(importer_path, clean, file_index, &[".h", ".hpp", ".hh"])
}

fn has_cpp_header_markers(text: &str) -> bool {
    [
        "namespace ",
        "template<",
        "template <",
        "class ",
        "constexpr",
        "std::",
        "::",
        "public:",
        "private:",
        "noexcept",
        "extern \"C\"",
        "virtual ",
        "typename",
        "nullptr",
        "operator",
        "friend ",
        "using ",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

/// True when the C grammar parses the header with no error. C++-only spellings the markers miss (`const T& x`)
/// leave an error here, and such a header stays on the C++ grammar, which parses C as well.
fn parses_as_c(source: &[u8]) -> bool {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .is_ok_and(|()| {
            parser
                .parse(source, None)
                .is_some_and(|tree| !tree.root_node().has_error())
        })
}

pub(crate) fn refine_c_header_dialect(
    base_lang: crate::domain::Lang,
    rel_path: &str,
    source_bytes: &[u8],
) -> crate::domain::Lang {
    if !rel_path.ends_with(".h") {
        return base_lang;
    }
    if let Ok(text) = std::str::from_utf8(source_bytes) {
        if text.contains("@interface ") || text.contains("@protocol ") || text.contains("@property")
        {
            return crate::domain::Lang::ObjC;
        }
        let c_base = matches!(base_lang, crate::domain::Lang::C | crate::domain::Lang::Cpp);
        if c_base && !has_cpp_header_markers(text) && parses_as_c(source_bytes) {
            return crate::domain::Lang::C;
        }
    }
    crate::domain::Lang::Cpp
}
