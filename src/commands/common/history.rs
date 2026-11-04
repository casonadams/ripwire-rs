//! Git-history runner shared by the cochange and hotspots metric commands.

use std::io;

pub fn execute_with_git_state_fmt<F>(
    root: &str,
    cutoff: Option<i64>,
    pretty: bool,
    json_out: bool,
    runner: F,
) -> io::Result<()>
where
    F: FnOnce(
        &[crate::git::history::CommitInfo],
        &crate::Workspace,
        &mut crate::serialize::XmlWriter<'_, io::StdoutLock<'_>>,
    ) -> io::Result<()>,
{
    let commits_vec = crate::git::history::walk_repository_history_scoped(root, cutoff)
        .map_err(io::Error::other)?;
    let commits = commits_vec.as_slice();
    let state = crate::Workspace::load(root)
        .ok_or_else(|| io::Error::other("failed to load repository"))?;
    let mut stdout_lock = io::stdout().lock();
    let mut stdout = crate::serialize::XmlWriter::with_format(&mut stdout_lock, pretty, json_out);
    runner(commits, &state, &mut stdout)?;
    stdout.finish()
}
