//! Changed-file discovery shared by the safety and handoff commands.

use std::io;

/// Tracked files that differ from HEAD; `Err` when the working tree status cannot be read.
pub fn get_working_tree_changed_files(root: &str) -> io::Result<Vec<String>> {
    crate::git::repo::working_tree_changes(root).map_err(io::Error::other)
}

pub fn resolve_changed_or_spec_files(
    root: &str,
    files_spec: Option<&str>,
) -> io::Result<Vec<String>> {
    match files_spec {
        Some(f) => Ok(f
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()),
        None => get_working_tree_changed_files(root),
    }
}
