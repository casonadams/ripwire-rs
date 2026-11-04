#[must_use]
pub fn revision_tree<'a>(repo: &'a gix::Repository, rev: &str) -> Option<gix::Tree<'a>> {
    repo.rev_parse_single(rev)
        .ok()?
        .object()
        .ok()?
        .peel_to_tree()
        .ok()
}

/// Tree of `rev`; `Err` when `rev` does not resolve to a tree.
pub(super) fn rev_tree<'r>(repo: &'r gix::Repository, rev: &str) -> Result<gix::Tree<'r>, String> {
    let id = repo
        .rev_parse_single(rev)
        .map_err(|e| format!("'{rev}': {e}"))?;
    let object = id.object().map_err(|e| format!("'{rev}': {e}"))?;
    object.peel_to_tree().map_err(|e| format!("'{rev}': {e}"))
}

pub fn revision_commit<'a>(repo: &'a gix::Repository, rev: &str) -> Option<gix::Commit<'a>> {
    repo.rev_parse_single(rev)
        .ok()?
        .object()
        .ok()?
        .peel_to_commit()
        .ok()
}

/// Id of `HEAD`'s commit; `Ok(None)` when `HEAD` is unborn.
pub fn head_commit_id(repo: &gix::Repository) -> Result<Option<gix::ObjectId>, String> {
    let mut head = repo.head().map_err(|e| format!("HEAD: {e}"))?;
    if head.is_unborn() {
        return Ok(None);
    }
    Ok(Some(
        head.peel_to_commit().map_err(|e| format!("HEAD: {e}"))?.id,
    ))
}

fn branch_merge_base(
    repo: &gix::Repository,
    branch: &str,
) -> Result<Option<(gix::ObjectId, gix::ObjectId)>, String> {
    let Some(tip) = revision_commit(repo, branch) else {
        return Ok(None);
    };
    let Some(head) = head_commit_id(repo)? else {
        return Ok(None);
    };
    let base = repo
        .merge_base(head, tip.id)
        .map_err(|e| format!("merge base of '{branch}': {e}"))?;
    Ok(base.map(|base| (base.detach(), tip.id)))
}

/// Merge base of `HEAD` and `branch`; `Ok(None)` when they share no history or `HEAD` is unborn.
pub fn merge_base_id(
    repo: &gix::Repository,
    branch: &str,
) -> Result<Option<gix::ObjectId>, String> {
    Ok(branch_merge_base(repo, branch)?.map(|(base, _)| base))
}

/// The base of a `REV`, `REV..HEAD` or `REV...HEAD` spec. The compared side is always the working
/// tree, so a range whose right side is not `HEAD` is refused rather than silently reinterpreted.
pub fn base_of_range(spec: &str) -> Result<&str, String> {
    let Some((base, target)) = spec.split_once("..") else {
        return Ok(spec);
    };
    let target = target.strip_prefix('.').unwrap_or(target);
    if !target.is_empty() && target != "HEAD" {
        return Err(format!(
            "'{spec}': only REV or REV..HEAD is supported; the compared side is the working tree"
        ));
    }
    if base.is_empty() {
        return Err(format!("'{spec}': missing base ref"));
    }
    Ok(base)
}

/// Object id that `rev` resolves to, or `None` when it does not resolve.
#[must_use]
pub fn rev_id(repo: &gix::Repository, rev: &str) -> Option<gix::ObjectId> {
    repo.rev_parse_single(rev).ok().map(gix::Id::detach)
}
