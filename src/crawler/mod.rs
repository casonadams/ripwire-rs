use crate::domain::Lang;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

pub const DEFAULT_MAX_FILE_SIZE: u64 = 4 * 1024 * 1024;
pub const DEFAULT_JSON_CEILING: u64 = 256 * 1024;
pub const DEFAULT_YAML_CEILING: u64 = 512 * 1024;
pub const DENYLIST_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "vendor",
    "third_party",
    ".cache",
    "build",
    "dist",
    "out",
    "target",
    ".venv",
    "venv",
    "__pycache__",
    ".idea",
    ".vscode",
    "asan",
    "build_prof",
    "CMakeFiles",
    "captures",
    ".agents",
    ".claude",
    ".skills",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrawledFile {
    pub path: PathBuf,
    pub rel_path: String,
    pub lang: Lang,
    pub byte_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CrawlerOptions {
    pub max_file_size: u64,
    pub honor_gitignore: bool,
    pub ignore_tests: bool,
    pub custom_excludes: Vec<String>,
}

impl CrawlerOptions {
    /// Cache identity suffix: empty for the defaults, so their cache keys and files are unchanged;
    /// otherwise a hash of the options with excludes order-insensitive.
    #[must_use]
    pub fn cache_tag(&self) -> String {
        use std::hash::{Hash, Hasher};
        if *self == Self::default() {
            return String::new();
        }
        let mut normalized = self.clone();
        normalized.custom_excludes.sort_unstable();
        normalized.custom_excludes.dedup();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        normalized.hash(&mut hasher);
        format!("-o{:x}", hasher.finish())
    }
}

impl Default for CrawlerOptions {
    fn default() -> Self {
        Self {
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            honor_gitignore: true,
            ignore_tests: false,
            custom_excludes: Vec::new(),
        }
    }
}

fn build_walker(root: &Path, options: &CrawlerOptions) -> ignore::Walk {
    let custom_excludes = options.custom_excludes.clone();
    let mut builder = WalkBuilder::new(root);
    builder
        .git_ignore(options.honor_gitignore)
        .git_global(options.honor_gitignore)
        .git_exclude(options.honor_gitignore)
        .hidden(false)
        .follow_links(false)
        .filter_entry(move |entry| {
            if entry.depth() == 0 {
                return true;
            }
            let Some(name) = entry.file_name().to_str() else {
                return true;
            };
            let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());
            !is_entry_denylisted(name, is_dir, &custom_excludes)
        })
        .build()
}

fn resolve_path_lang(path: &Path) -> Lang {
    Lang::from_path(path)
}

fn process_crawl_entry(
    entry: &ignore::DirEntry,
    root: &Path,
    options: &CrawlerOptions,
) -> Option<CrawledFile> {
    let path = entry.path();
    if path.is_dir() {
        return None;
    }
    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
    let rel_path = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let mut file = evaluate_candidate(&rel_path, size, false, options)?;
    file.path = path.to_path_buf();
    Some(file)
}

fn tracked_path(root: &Path, relative: &Path) -> Option<PathBuf> {
    let mut path = root.to_path_buf();
    for part in relative.components() {
        path.push(part);
        if path.is_symlink() {
            return None;
        }
    }
    Some(path)
}

fn tracked_file(root: &Path, relative: &str, options: &CrawlerOptions) -> Option<(PathBuf, u64)> {
    let relative_path = Path::new(relative);
    if !relative_path
        .components()
        .all(|part| matches!(part, std::path::Component::Normal(_)))
        || is_path_denylisted(relative_path, &options.custom_excludes)
    {
        return None;
    }
    let path = tracked_path(root, relative_path)?;
    let metadata = path.symlink_metadata().ok()?;
    metadata.is_file().then_some((path, metadata.len()))
}

fn append_tracked_files(root: &Path, options: &CrawlerOptions, files: &mut Vec<CrawledFile>) {
    let Some(root_str) = root.to_str() else {
        return;
    };
    let mut added = Vec::new();
    crate::git::repo::for_each_tracked_path(root_str, |relative| {
        if files
            .binary_search_by(|file| file.rel_path.as_str().cmp(relative))
            .is_ok()
        {
            return;
        }
        let Some((path, size)) = tracked_file(root, relative, options) else {
            return;
        };
        if let Some(mut file) = evaluate_candidate(relative, size, false, options) {
            file.path = path;
            added.push(file);
        }
    });
    files.extend(added);
    files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    files.dedup_by(|a, b| a.rel_path == b.rel_path);
}

fn append_tracked_paths(root: &Path, options: &CrawlerOptions, paths: &mut Vec<String>) {
    let Some(root_str) = root.to_str() else {
        return;
    };
    let mut added = Vec::new();
    crate::git::repo::for_each_tracked_path(root_str, |relative| {
        if paths
            .binary_search_by(|path| path.as_str().cmp(relative))
            .is_err()
            && tracked_file(root, relative, options).is_some()
        {
            added.push(relative.to_string());
        }
    });
    paths.extend(added);
    paths.sort_unstable();
    paths.dedup();
}

/// Evaluate a file passed directly as the crawl root: language, size and generated-name rules apply
/// to its file name, the test-path rule to the path as typed; directory denylists never apply.
#[must_use]
pub fn single_file_candidate(
    path: &Path,
    byte_size: u64,
    options: &CrawlerOptions,
) -> Option<CrawledFile> {
    let name = path.file_name()?.to_str()?;
    let typed = path.to_string_lossy().replace('\\', "/");
    if options.ignore_tests && crate::gate::affected::is_test_path(&typed) {
        return None;
    }
    let mut file = evaluate_candidate(name, byte_size, false, options)?;
    file.rel_path = typed;
    file.path = path.to_path_buf();
    Some(file)
}

fn crawl_single_file(path: &Path, options: &CrawlerOptions) -> Vec<CrawledFile> {
    path.metadata()
        .ok()
        .and_then(|metadata| single_file_candidate(path, metadata.len(), options))
        .into_iter()
        .collect()
}

pub fn crawl_directory<P: AsRef<Path>>(root: P, options: &CrawlerOptions) -> Vec<CrawledFile> {
    let root = root.as_ref();
    if root.is_file() {
        return crawl_single_file(root, options);
    }
    let mut files = Vec::new();
    for entry in build_walker(root, options).flatten() {
        if let Some(file) = process_crawl_entry(&entry, root, options) {
            files.push(file);
        }
    }
    files.sort_by(|a, b| a.rel_path.as_bytes().cmp(b.rel_path.as_bytes()));
    append_tracked_files(root, options, &mut files);
    files
}

#[inline]
fn is_entry_denylisted(name: &str, is_dir: bool, custom_excludes: &[String]) -> bool {
    if is_dir {
        DENYLIST_DIRS.contains(&name)
            || name.starts_with("cmake-build-")
            || custom_excludes.iter().any(|ex| name.contains(ex))
    } else {
        custom_excludes.iter().any(|ex| name.contains(ex))
    }
}

/// Check if a path component is in the default or custom denylist.
#[must_use]
pub fn is_path_denylisted<P: AsRef<Path>>(path: P, custom_excludes: &[String]) -> bool {
    path.as_ref().components().any(|c| {
        c.as_os_str().to_str().is_some_and(|name| {
            DENYLIST_DIRS.contains(&name)
                || name.starts_with("cmake-build-")
                || custom_excludes.iter().any(|ex| name.contains(ex))
        })
    })
}

/// Check if a buffer contains null bytes (binary file heuristic).
#[inline]
#[must_use]
pub fn is_binary_buffer(buf: &[u8]) -> bool {
    buf.contains(&0)
}

/// Pure in-memory evaluation of a candidate file path, language, and size.
#[must_use]
pub fn evaluate_candidate(
    rel_path: &str,
    byte_size: u64,
    is_binary: bool,
    options: &CrawlerOptions,
) -> Option<CrawledFile> {
    let path = Path::new(rel_path);
    if is_path_denylisted(path, &options.custom_excludes)
        || (options.ignore_tests && crate::gate::affected::is_test_path(rel_path))
    {
        return None;
    }
    let lang = resolve_path_lang(path);
    let ceiling = lang.semantics().max_file_size(options.max_file_size);
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if lang == Lang::Unknown
        || byte_size == 0
        || byte_size > ceiling
        || is_binary
        || lang.semantics().is_generated_file_name(file_name)
    {
        return None;
    }
    Some(CrawledFile {
        path: path.to_path_buf(),
        rel_path: rel_path.replace('\\', "/"),
        lang,
        byte_size,
    })
}

#[derive(Debug, Default, Clone)]
pub struct SkippedCensus {
    pub oversize: Vec<(String, u64, String)>,
    pub unsupported: Vec<(String, u64, String)>,
    pub excluded: Vec<(String, u64, String)>,
    pub unindexed_exts: Vec<(String, usize)>,
}

pub fn classify_skipped_item(
    rel_path: &str,
    is_dir: bool,
    size: u64,
    options: &CrawlerOptions,
    census: &mut SkippedCensus,
    ext_counts: &mut std::collections::HashMap<String, usize>,
) {
    if is_dir {
        return;
    }
    let norm_path = rel_path.replace('\\', "/");
    let ext = Path::new(&norm_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if is_path_denylisted(&norm_path, &options.custom_excludes) {
        census.excluded.push((norm_path, size, ext));
        return;
    }
    let lang = resolve_path_lang(Path::new(&norm_path));
    if lang == Lang::Unknown {
        if !ext.is_empty() {
            *ext_counts.entry(ext.clone()).or_default() += 1;
        }
        census.unsupported.push((norm_path, size, ext));
    } else if size > lang.semantics().max_file_size(options.max_file_size) {
        census.oversize.push((norm_path, size, ext));
    }
}

fn classify_skipped_entry(
    entry: &ignore::DirEntry,
    root: &Path,
    options: &CrawlerOptions,
    census: &mut SkippedCensus,
    ext_counts: &mut std::collections::HashMap<String, usize>,
) {
    let path = entry.path();
    let size = entry.metadata().map_or(0, |m| m.len());
    let rel_path = if path == root {
        path.file_name().unwrap_or_default()
    } else {
        path.strip_prefix(root).unwrap_or(path).as_os_str()
    };
    classify_skipped_item(
        &rel_path.to_string_lossy(),
        path.is_dir(),
        size,
        options,
        census,
        ext_counts,
    );
}

#[must_use]
pub fn scan_skipped_census<P: AsRef<Path>>(root: P, options: &CrawlerOptions) -> SkippedCensus {
    let root = root.as_ref();
    let mut census = SkippedCensus::default();
    let mut ext_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for entry in build_walker(root, options).flatten() {
        classify_skipped_entry(&entry, root, options, &mut census, &mut ext_counts);
    }
    census.oversize.sort_by(|a, b| a.0.cmp(&b.0));
    census.unsupported.sort_by(|a, b| a.0.cmp(&b.0));
    census.excluded.sort_by(|a, b| a.0.cmp(&b.0));
    let mut exts: Vec<(String, usize)> = ext_counts.into_iter().collect();
    exts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    census.unindexed_exts = exts;
    census
}

#[derive(Debug, Default, Clone)]
pub struct CorpusWalk {
    pub rel_paths: Vec<String>,
    pub pruned_dirs: usize,
}

fn count_denylisted_children(dir: &Path, custom_excludes: &[String]) -> usize {
    std::fs::read_dir(dir).map_or(0, |entries| {
        entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| is_entry_denylisted(n, true, custom_excludes))
            })
            .count()
    })
}

#[must_use]
pub fn walk_corpus<P: AsRef<Path>>(root: P, options: &CrawlerOptions) -> CorpusWalk {
    let root = root.as_ref();
    let mut walk = CorpusWalk::default();
    for entry in build_walker(root, options).flatten() {
        let Some(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_dir() {
            walk.pruned_dirs += count_denylisted_children(entry.path(), &options.custom_excludes);
        } else if ft.is_file() {
            let rel = entry.path().strip_prefix(root).unwrap_or(entry.path());
            walk.rel_paths
                .push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    walk.rel_paths.sort_unstable();
    append_tracked_paths(root, options, &mut walk.rel_paths);
    walk
}
