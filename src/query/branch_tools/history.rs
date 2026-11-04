use serde_json::{Value, json};

/// The `fate` of a name from its newest count-changing commit. An unreadable history is
/// `unavailable` with the reason, never `never`.
pub fn name_fate(last: Result<Option<crate::git::repo::CommitSummary>, String>) -> Value {
    match last {
        Ok(Some(commit)) => json!({
            "v": "removed",
            "commit": commit.short_id,
            "date": commit.date,
            "subject": commit.subject
        }),
        Ok(None) => json!({
            "v": "never",
            "note": "no commit reachable from HEAD ever removed a line carrying this name \u{2014} so for a name HEAD does not have, this repo never had it"
        }),
        Err(reason) => json!({ "v": "unavailable", "note": reason }),
    }
}

pub(in crate::query) fn probe_name_history(
    root: &str,
    sym_name: &str,
    head_sha: &str,
) -> Option<Value> {
    let repo = crate::git::repo::discover(root)?;
    let fate = name_fate(crate::git::repo::last_count_change(&repo, sym_name));
    Some(json!({ "probed": 1, "head": head_sha, "fate": fate }))
}
