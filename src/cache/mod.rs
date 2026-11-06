use crate::domain::{IngestResult, Interner};
use crate::graph::Graph;
use crate::rank::PageRankResult;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Blob layout and serialized-type version. A blob of any other version is a miss, never parsed.
pub const CACHE_VERSION: u32 = 39;

/// Every blob starts with these four bytes, then `CACHE_VERSION` as a little-endian `u32`; the
/// postcard body follows.
const BLOB_MAGIC: &[u8; 4] = b"RWCB";
const BLOB_HEADER_LEN: usize = 8;

/// Cached repository analysis. postcard writes fields in declaration order and no names, so that
/// order is the layout.
#[derive(Serialize, Deserialize)]
pub struct CachePayload {
    pub validation_key: String,
    pub interner_strings: Vec<String>,
    pub ingest: IngestResult,
    pub graph: Graph,
    pub pr: PageRankResult,
}

#[derive(Serialize)]
struct CachePayloadRef<'a> {
    validation_key: String,
    interner_strings: Vec<String>,
    ingest: &'a IngestResult,
    graph: &'a Graph,
    pr: &'a PageRankResult,
}

/// Compute a fast cache validation key from git HEAD (or mtime metadata).
#[must_use]
fn current_binary_stamp() -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    if let Ok(exe) = std::env::current_exe()
        && let Ok(meta) = fs::metadata(exe)
    {
        if let Ok(mtime) = meta.modified() {
            mtime.hash(&mut hasher);
        }
        meta.len().hash(&mut hasher);
    }
    hasher.finish()
}

fn hash_dir_entries(path: &Path, hasher: &mut impl Hasher) {
    for entry in ignore::WalkBuilder::new(path)
        .sort_by_file_path(Ord::cmp)
        .build()
        .flatten()
    {
        entry.path().hash(hasher);
        if let Ok(meta) = entry.metadata() {
            if let Ok(mtime) = meta.modified() {
                mtime.hash(hasher);
            }
            meta.len().hash(hasher);
        }
    }
}
/// A dirty path with its on-disk stat; `None` when the file no longer exists.
pub type DirtyEntry = (String, Option<(std::time::SystemTime, u64)>);

/// Key suffix identifying uncommitted working-tree state; empty for a clean tree so clean keys stay stable.
#[must_use]
pub fn dirty_key_suffix(entries: &[DirtyEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    entries.hash(&mut hasher);
    format!("-d{:x}", hasher.finish())
}

fn dirty_entries(root: &str) -> io::Result<Vec<DirtyEntry>> {
    let paths = crate::git::repo::dirty_paths(root).map_err(io::Error::other)?;
    Ok(paths
        .into_iter()
        .map(|rel| {
            let stat = fs::metadata(Path::new(root).join(&rel))
                .ok()
                .and_then(|meta| Some((meta.modified().ok()?, meta.len())));
            (rel, stat)
        })
        .collect())
}

/// Validation key for `root`. `Err` when the working tree cannot be read: dirtiness is then unknown,
/// so no key may claim the tree is clean.
pub fn compute_cache_key(root: &str) -> io::Result<String> {
    let bin_stamp = current_binary_stamp();
    let path = Path::new(root);
    if let Ok(repo) = gix::open(path)
        && let Ok(head) = repo.head_commit()
    {
        let dirty = dirty_key_suffix(&dirty_entries(root)?);
        return Ok(format!(
            "{}-{bin_stamp:x}-{}{dirty}",
            env!("CARGO_PKG_VERSION"),
            head.id()
        ));
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bin_stamp.hash(&mut hasher);
    root.hash(&mut hasher);
    hash_dir_entries(path, &mut hasher);
    Ok(format!("mtime-{:x}", hasher.finish()))
}

/// Derive standard auto-cache path in TMPDIR for a target repository root.
#[must_use]
pub fn auto_cache_path(root: &str) -> PathBuf {
    tagged_auto_cache_path(root, "")
}

fn tagged_auto_cache_path(root: &str, tag: &str) -> PathBuf {
    let abs = fs::canonicalize(root).unwrap_or_else(|_| PathBuf::from(root));
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    abs.to_string_lossy().hash(&mut hasher);
    std::env::temp_dir().join(format!("ripwire-rs-cache-{:x}{tag}.bin", hasher.finish()))
}

/// The cache file and validation key for `root` under crawler options `tag` (empty for defaults).
fn cache_identity(
    root: &str,
    cache_path: Option<&Path>,
    tag: &str,
) -> io::Result<(PathBuf, String)> {
    let path = cache_path.map_or_else(|| tagged_auto_cache_path(root, tag), Path::to_path_buf);
    Ok((path, format!("{}{tag}", compute_cache_key(root)?)))
}

/// Attempt to load cached repository analysis if validation key matches.
pub fn load_cached_analysis(
    root: &str,
    cache_path: Option<&Path>,
    options_tag: &str,
) -> Option<(IngestResult, Graph, PageRankResult, Interner)> {
    let (path, key) = cache_identity(root, cache_path, options_tag).ok()?;
    let bytes = fs::read(path).ok()?;
    load_cached_payload_slice(&bytes, &key)
}

/// Load and validate cached payload from a byte slice. Any blob that is not a current, complete
/// payload for `expected_key` is `None`.
pub fn load_cached_payload_slice(
    bytes: &[u8],
    expected_key: &str,
) -> Option<(IngestResult, Graph, PageRankResult, Interner)> {
    let payload = decode_blob(bytes)?;
    if payload.validation_key != expected_key {
        return None;
    }
    let interner = Interner::from_strings(&payload.interner_strings);
    Some((payload.ingest, payload.graph, payload.pr, interner))
}

/// The postcard body of `bytes` when its header is this build's magic and `CACHE_VERSION`.
fn blob_body(bytes: &[u8]) -> Option<&[u8]> {
    let header = bytes.get(..BLOB_HEADER_LEN)?;
    let version = CACHE_VERSION.to_le_bytes();
    (header[..4] == BLOB_MAGIC[..] && header[4..] == version[..]).then(|| &bytes[BLOB_HEADER_LEN..])
}

/// The whole payload, or `None` for a stale header, a truncated body, or trailing bytes.
fn decode_blob(bytes: &[u8]) -> Option<CachePayload> {
    let (payload, rest) = postcard::take_from_bytes::<CachePayload>(blob_body(bytes)?).ok()?;
    rest.is_empty().then_some(payload)
}

/// Format state of blob `bytes`: `ok`, `cache-version` (a JSON blob from an older build, or this
/// build's magic with another version), or `corrupt`.
#[must_use]
pub fn blob_state(bytes: &[u8]) -> &'static str {
    if decode_blob(bytes).is_some() {
        return "ok";
    }
    let version = CACHE_VERSION.to_le_bytes();
    let legacy_json = bytes.first() == Some(&b'{');
    let stale = bytes.len() >= BLOB_HEADER_LEN
        && bytes[..4] == BLOB_MAGIC[..]
        && bytes[4..BLOB_HEADER_LEN] != version[..];
    if legacy_json || stale {
        "cache-version"
    } else {
        "corrupt"
    }
}

/// Read a blob's bytes from `reader` and classify them with [`blob_state`]; a read error is `corrupt`.
#[must_use]
pub fn probe_blob<R: Read>(mut reader: R) -> &'static str {
    let mut bytes = Vec::new();
    match reader.read_to_end(&mut bytes) {
        Ok(_) => blob_state(&bytes),
        Err(_) => "corrupt",
    }
}

/// The blob bytes of `payload`: the layout [`save_cached_analysis`] writes and
/// [`load_cached_payload_slice`] reads.
pub fn encode_payload(payload: &CachePayload) -> io::Result<Vec<u8>> {
    encode_blob(payload)
}

fn encode_blob<T: Serialize>(payload: &T) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(BLOB_HEADER_LEN);
    bytes.extend_from_slice(BLOB_MAGIC);
    bytes.extend_from_slice(&CACHE_VERSION.to_le_bytes());
    postcard::to_extend(payload, bytes).map_err(io::Error::other)
}

/// Save repository analysis to cache file atomically.
pub fn save_cached_analysis(
    root: &str,
    cache_path: Option<&Path>,
    options_tag: &str,
    analysis: (&IngestResult, &Graph, &PageRankResult, &Interner),
) -> io::Result<()> {
    let (ingest, graph, pr, interner) = analysis;
    let (path, validation_key) = cache_identity(root, cache_path, options_tag)?;
    let payload = CachePayloadRef {
        validation_key,
        interner_strings: interner.all_strings(),
        ingest,
        graph,
        pr,
    };
    let bytes = encode_blob(&payload)?;

    let tmp_path = path.with_extension("tmp");
    let mut file = File::create(&tmp_path)?;
    file.write_all(&bytes)?;
    drop(file);
    fs::rename(tmp_path, &path)
}
