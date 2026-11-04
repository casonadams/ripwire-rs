const SHORT_ID_LEN: usize = 9;

#[must_use]
pub fn discover(root: &str) -> Option<gix::Repository> {
    gix::discover(root).ok()
}

/// The repository enclosing `root`: `Ok(None)` when no repository encloses it, `Err` when one is found but cannot be opened.
pub fn enclosing_repo(root: &str) -> Result<Option<gix::Repository>, String> {
    match gix::discover(root) {
        Ok(repo) => Ok(Some(repo)),
        Err(e) if e.is_not_found() => Ok(None),
        Err(e) => Err(format!("{root}: {e}")),
    }
}

#[must_use]
pub fn short_id(id: impl AsRef<gix::oid>) -> String {
    id.as_ref().to_hex_with_len(SHORT_ID_LEN).to_string()
}

/// `root`'s path inside the work tree as `dir/` (empty at the top), like `git rev-parse --show-prefix`.
pub fn workdir_prefix(repo: &gix::Repository, root: &str) -> Result<String, String> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| format!("{root} has no work tree"))?;
    let workdir = std::fs::canonicalize(workdir).map_err(|e| format!("{root}: {e}"))?;
    let canon_root = std::fs::canonicalize(root).map_err(|e| format!("{root}: {e}"))?;
    relative_prefix(&workdir, &canon_root).ok_or_else(|| format!("{root} is outside its work tree"))
}

/// `root` relative to `workdir` as `dir/` (empty at the top); `None` when `root` is outside `workdir`.
#[must_use]
pub fn relative_prefix(workdir: &std::path::Path, root: &std::path::Path) -> Option<String> {
    let rel = root
        .strip_prefix(workdir)
        .ok()?
        .to_str()?
        .replace('\\', "/");
    Some(if rel.is_empty() {
        rel
    } else {
        format!("{rel}/")
    })
}

/// Short names of `refs/heads/*`, sorted like `git for-each-ref`. `Err` when the refs cannot be read.
pub fn local_branches(repo: &gix::Repository) -> Result<Vec<String>, String> {
    let refs = repo.references().map_err(|e| format!("references: {e}"))?;
    let iter = refs
        .local_branches()
        .map_err(|e| format!("local branches: {e}"))?;
    let mut names = Vec::new();
    for reference in iter {
        let reference = reference.map_err(|e| format!("local branch: {e}"))?;
        names.push(reference.name().shorten().to_string());
    }
    names.sort_unstable();
    Ok(names)
}

/// Checked-out branch short name, or `HEAD` when detached (`git rev-parse --abbrev-ref HEAD`).
#[must_use]
pub fn head_ref(repo: &gix::Repository) -> String {
    repo.head_name()
        .ok()
        .flatten()
        .map_or_else(|| "HEAD".to_string(), |n| n.shorten().to_string())
}
