use super::super::{
    needs_lexical_normalization, normalize_lexical_path, probe_with_suffixes, strip_import_quotes,
};
use std::collections::HashMap;
use std::path::Path;

/// The directory a non-root module file's child modules live in (`src/git/repo.rs` declares `src/git/repo/`).
fn rust_child_module_dir(importer_path: &str) -> Option<String> {
    let path = Path::new(importer_path);
    let stem = path.file_stem()?.to_str()?;
    if matches!(stem, "mod" | "lib" | "main") {
        return None;
    }
    let dir = path.parent()?.to_str()?;
    Some(if dir.is_empty() {
        stem.to_string()
    } else {
        format!("{dir}/{stem}")
    })
}

fn find_rust_crate_src_dir(importer_path: &str) -> &str {
    if let Some(pos) = importer_path.rfind("/src/") {
        &importer_path[..pos + 4]
    } else if importer_path.starts_with("src/") {
        "src"
    } else {
        Path::new(importer_path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("")
    }
}

fn resolve_rust_super_dir(importer_path: &str) -> &str {
    let p = Path::new(importer_path);
    let is_mod_root = matches!(
        p.file_name().and_then(|n| n.to_str()),
        Some("mod.rs" | "lib.rs" | "main.rs")
    );
    let dir = if is_mod_root {
        p.parent().and_then(|d| d.parent())
    } else {
        p.parent()
    };
    dir.and_then(|d| d.to_str()).unwrap_or("")
}

fn split_rust_anchor(clean: &str) -> Option<(&str, &str)> {
    for anchor in ["crate", "self", "super"] {
        if clean == anchor {
            return Some((anchor, ""));
        }
        if let Some(rest) = clean
            .strip_prefix(anchor)
            .and_then(|s| s.strip_prefix("::"))
        {
            return Some((anchor, rest));
        }
    }
    None
}

fn resolve_rust_base_and_rest<'a>(
    importer_path: &'a str,
    clean: &'a str,
) -> Option<(&'a str, &'a str)> {
    let (anchor, rest) = split_rust_anchor(clean)?;
    let base = match anchor {
        "crate" => find_rust_crate_src_dir(importer_path),
        "self" => Path::new(importer_path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(""),
        _ => resolve_rust_super_dir(importer_path),
    };
    Some((base, rest))
}

fn probe_rust_segs_in_base(
    base_dir: &str,
    segs: &[&str],
    file_index: &HashMap<&str, u32>,
) -> Option<u32> {
    for take_len in [segs.len(), segs.len().saturating_sub(1)] {
        if take_len == 0 {
            continue;
        }
        let mut cand = String::new();
        if !base_dir.is_empty() {
            cand.push_str(base_dir);
            cand.push('/');
        }
        cand.push_str(&segs[..take_len].join("/"));
        if needs_lexical_normalization(&cand) {
            cand = normalize_lexical_path("", &cand);
        }
        if let Some(id) = probe_with_suffixes(&cand, file_index, &[".rs", "/mod.rs"]) {
            return Some(id);
        }
    }
    None
}

fn resolve_rust_workspace_crate_target(
    clean: &str,
    file_index: &HashMap<&str, u32>,
) -> Option<u32> {
    let segs: Vec<&str> = clean.split("::").filter(|s| !s.is_empty()).collect();
    if segs.len() < 2 {
        return None;
    }
    let crate_dir = format!("crates/{}/src", segs[0].replace('_', "-"));
    probe_rust_segs_in_base(&crate_dir, &segs[1..], file_index)
}

pub(super) fn resolve_rust_use_target(
    importer_path: &str,
    raw_target: &str,
    file_index: &HashMap<&str, u32>,
) -> Option<u32> {
    let clean = strip_import_quotes(raw_target)
        .split('{')
        .next()
        .unwrap_or("")
        .trim_end_matches("::*")
        .trim_end_matches("::");
    let segs: Vec<&str> = clean.split("::").filter(|s| !s.is_empty()).collect();
    if let Some((base_dir, rest)) = resolve_rust_base_and_rest(importer_path, clean) {
        let rest_segs: Vec<&str> = rest.split("::").filter(|s| !s.is_empty()).collect();
        if let Some(id) = probe_rust_segs_in_base(base_dir, &rest_segs, file_index) {
            return Some(id);
        }
        // `super::item` and `super::*` name the parent module itself when the item is not a file of its own.
        if clean == "super" || clean.starts_with("super::") {
            return probe_with_suffixes(base_dir, file_index, &[".rs", "/mod.rs"]);
        }
        return None;
    }
    let self_dir = Path::new(importer_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("");
    // A non-root module file (`repo.rs`) declares its children in its own directory (`repo/`), and a
    // bare `use child::item` names them before anything beside the file.
    let child_dir = rust_child_module_dir(importer_path);
    let bases = child_dir
        .iter()
        .map(String::as_str)
        .chain([self_dir, find_rust_crate_src_dir(importer_path)]);
    for base in bases {
        if let Some(id) = probe_rust_segs_in_base(base, &segs, file_index) {
            return Some(id);
        }
    }
    resolve_rust_workspace_crate_target(clean, file_index)
        .or_else(|| resolve_rust_unique_suffix(&segs, file_index))
}

/// A path led by a crate name the index cannot map (`pkg::commands::x`) resolves to the one indexed file
/// whose path ends in its module segments after that name. Two candidates, or a std path, resolve to nothing.
fn resolve_rust_unique_suffix(segs: &[&str], file_index: &HashMap<&str, u32>) -> Option<u32> {
    let [head, tail @ ..] = segs else {
        return None;
    };
    if matches!(*head, "std" | "core" | "alloc") || tail.is_empty() {
        return None;
    }
    for take in [tail.len(), tail.len() - 1] {
        if take == 0 {
            continue;
        }
        let module = tail[..take].join("/");
        let mut hits = file_index
            .iter()
            .filter(|(path, _)| path_is_module_file(path, &module))
            .map(|(_, id)| *id);
        if let Some(first) = hits.next() {
            return hits.next().is_none().then_some(first);
        }
    }
    None
}

fn path_is_module_file(path: &str, module: &str) -> bool {
    [path.strip_suffix(".rs"), path.strip_suffix("/mod.rs")]
        .into_iter()
        .flatten()
        .any(|stem| {
            stem.strip_suffix(module)
                .is_some_and(|rest| rest.is_empty() || rest.ends_with('/'))
        })
}
