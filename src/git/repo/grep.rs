use gix::bstr::ByteSlice;

use super::tree::{is_binary, sorted_blobs};

const MAX_WORD_BLOB_BYTES: u64 = 2 * 1024 * 1024;

/// One line of a blob that contains a searched string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineHit {
    pub path: String,
    pub line: usize,
    pub text: String,
    pub blob: gix::ObjectId,
}

#[derive(Debug)]
pub struct WordGrep {
    pub per_rev: Vec<Vec<LineHit>>,
    pub blobs: usize,
}

/// Match `git grep -n -w -F` within a `dir/` scope; paths are scope-relative.
/// Shared revision blobs are read once.
pub fn grep_word_revs(
    repo: &gix::Repository,
    revs: &[&str],
    scope: &str,
    word: &str,
) -> Result<WordGrep, String> {
    use rayon::prelude::*;
    let sync_repo = repo.clone().into_sync();
    let trees: Vec<Vec<(String, gix::ObjectId)>> = revs
        .par_iter()
        .map_init(
            || sync_repo.to_thread_local(),
            |r, rev| sorted_blobs(r, rev, scope),
        )
        .collect::<Result<Vec<_>, String>>()?;
    let mut oids: Vec<gix::ObjectId> = trees.iter().flatten().map(|(_, oid)| *oid).collect();
    oids.sort_unstable();
    oids.dedup();
    let blobs = oids.len();
    let lines: Vec<(gix::ObjectId, Vec<(usize, String)>)> = oids
        .into_par_iter()
        .map_init(
            || sync_repo.to_thread_local(),
            |r, oid| word_lines(r, oid, word).map(|found| (oid, found)),
        )
        .collect::<Result<Vec<_>, String>>()?;
    let by_blob: std::collections::HashMap<gix::ObjectId, Vec<(usize, String)>> = lines
        .into_iter()
        .filter(|(_, found)| !found.is_empty())
        .collect();
    let per_rev = trees
        .into_iter()
        .map(|tree| {
            tree.into_iter()
                .flat_map(|(path, oid)| {
                    by_blob
                        .get(&oid)
                        .into_iter()
                        .flatten()
                        .map(move |(line, text)| LineHit {
                            path: path.clone(),
                            line: *line,
                            text: text.clone(),
                            blob: oid,
                        })
                })
                .collect()
        })
        .collect();
    Ok(WordGrep { per_rev, blobs })
}

fn word_lines(
    repo: &gix::Repository,
    oid: gix::ObjectId,
    word: &str,
) -> Result<Vec<(usize, String)>, String> {
    let header = repo
        .find_header(oid)
        .map_err(|e| format!("object {oid}: {e}"))?;
    if header.size() > MAX_WORD_BLOB_BYTES {
        return Ok(Vec::new());
    }
    let blob = repo
        .find_blob(oid)
        .map_err(|e| format!("blob {oid}: {e}"))?;
    if is_binary(&blob.data) || blob.data.find(word).is_none() {
        return Ok(Vec::new());
    }
    Ok(blob
        .data
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let text = line.to_str_lossy();
            has_word(&text, word).then(|| (idx + 1, text.into_owned()))
        })
        .collect())
}

/// True when `word` occurs in `line` bounded by non-identifier characters.
#[must_use]
pub fn has_word(line: &str, word: &str) -> bool {
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let bytes = line.as_bytes();
    !word.is_empty()
        && line.match_indices(word).any(|(i, _)| {
            let end = i + word.len();
            (i == 0 || !is_ident(bytes[i - 1])) && (end == bytes.len() || !is_ident(bytes[end]))
        })
}
