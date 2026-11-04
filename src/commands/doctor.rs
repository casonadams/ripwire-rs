use super::plugin::report_command;
use crate::commands::context::ExecutionContext;
use crate::ingest::LanguageRegistry;
use crate::serialize::{XmlEscapeExt, XmlWriter};
use std::borrow::Cow;
use std::collections::{BTreeSet, HashSet};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const BLOB_SCAN_CAP: usize = 4096;
const MANY_BLOBS: usize = 50;
const TRACKED_SCAN_CAP: usize = 20_000;
const SNIFF_BYTES: u64 = 8000;
const AUTO_CACHE_PREFIX: &str = "ripwire-rs-cache-";
const AUTO_CACHE_SUFFIX: &str = ".json";

const DOCTOR_LEGEND: &str = "<!-- ripwire-rs doctor schema=ripwire-rs.doctor/v1: setup health: <c n= ok=> checks. checks=/passed=: rows emitted / rows with ok=1. n=: binary-path, grammars, cache-dir, git, tree-sitter, tracked-binaries, index-cache, git-config-trust. self=/which=: this binary / the same name found on PATH; which_version=: the PATH copy's version line, printed only when it is not this binary; on_path=0 fails the row and hint= carries the export line; same_file=1: both resolve to one file. loaded=/expected=: distinct compiled-in grammars ABI-compatible with the linked tree-sitter / distinct compiled-in grammars; a shortfall fails the row. dir=: the temp directory holding auto-cache blobs (ripwire-rs-cache-*.json); unwritable fails the row. blobs=/bytes=: blob count / total size; blobs_floor=1 when the scan stopped at 4096. many=1: more than 50 blobs, informational. truncated=1: scan cut short (cache-dir: cap or I/O error; tracked-binaries: over 20000 tracked files, binaries= omitted). volatile=: attributes reading live machine state. repo=0|1: root is inside a git repository; git is read in-process, so a git executable is needed only to clone a remote URL root. history=0|1: HEAD resolves to a commit; head= its 9-hex short sha. core_abi=/newest_grammar_abi=: tree-sitter language ABI / the newest compiled-in grammar ABI. languages=N: distinct compiled-in grammars. tracked=N: index entries under root. binaries=N: tracked files whose first 8000 bytes hold a NUL. non_git=1: no repository; the row passes unscanned. cache_version=: index cache format version. source=auto|cache-flag|disabled: the index artifact this run reads (per-root auto-cache, the file named by the cache flag, or none when the cache is disabled). lean_path=: the index artifact checked: the file named by the cache flag or the auto-cache for root. lean=: ok, absent, cache-version (other format), corrupt or unreadable; only the last two fail the row. fsmonitor=: core.fsmonitor: unset, builtin, off or hook (a command git runs). neutralised=1: a hook is configured but never runs: ripwire-rs reads git in-process, and its one git invocation (cloning a remote URL root) passes -c core.fsmonitor=false. filter_drivers=N: filter drivers the repository's own config defines. -->\n";

/// One `<c>` row of the doctor report: a named check, its verdict, and measured attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorCheck {
    pub name: &'static str,
    pub ok: bool,
    pub attrs: Vec<(&'static str, String)>,
}

impl DoctorCheck {
    #[must_use]
    pub fn new(name: &'static str, ok: bool) -> Self {
        Self {
            name,
            ok,
            attrs: Vec::new(),
        }
    }

    #[must_use]
    pub fn attr(mut self, key: &'static str, value: impl ToString) -> Self {
        self.attrs.push((key, value.to_string()));
        self
    }

    fn flag(self, key: &'static str, value: bool) -> Self {
        self.attr(key, u8::from(value))
    }

    pub fn write_xml<W: Write>(&self, out: &mut W) -> io::Result<()> {
        write!(out, "<c n=\"{}\" ok=\"{}\"", self.name, u8::from(self.ok))?;
        for (key, value) in &self.attrs {
            write!(out, " {key}=\"{}\"", value.xml_escaped())?;
        }
        out.write_all(b"/>")
    }
}

/// Doctor envelope: the git stamp, the build identity, and the ordered check rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReport {
    pub at: Result<Option<String>, String>,
    pub built_from: String,
    pub checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    #[must_use]
    pub fn passed(&self) -> usize {
        self.checks.iter().filter(|c| c.ok).count()
    }

    pub fn write_xml<W: Write>(&self, out: &mut W) -> io::Result<()> {
        out.write_all(DOCTOR_LEGEND.as_bytes())?;
        let at_attr = crate::serialize::git_at_attr(self.at.clone());
        write!(
            out,
            "<doctor schema=\"ripwire-rs.doctor/v1\" checks=\"{}\" passed=\"{}\"{at_attr} built_from=\"{}\">",
            self.checks.len(),
            self.passed(),
            self.built_from.xml_escaped()
        )?;
        for check in &self.checks {
            check.write_xml(out)?;
        }
        out.write_all(b"</doctor>\n")
    }
}

/// Classify a `core.fsmonitor` value the way git interprets it.
#[must_use]
pub fn classify_fsmonitor(value: Option<&str>) -> &'static str {
    let Some(raw) = value else {
        return "unset";
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => "builtin",
        "false" | "no" | "off" | "0" | "" => "off",
        _ => "hook",
    }
}

/// True for file names the auto-cache writes (`ripwire-rs-cache-<hash>.json`).
#[must_use]
pub fn is_cache_blob_name(name: &str) -> bool {
    name.len() > AUTO_CACHE_PREFIX.len() + AUTO_CACHE_SUFFIX.len()
        && name.starts_with(AUTO_CACHE_PREFIX)
        && name.ends_with(AUTO_CACHE_SUFFIX)
}

#[derive(serde::Deserialize)]
struct CacheVersionProbe {
    version: u32,
}

/// Classify a cache artifact's format from its bytes: `ok`, `cache-version`, or `corrupt`.
#[must_use]
pub fn probe_cache_artifact<R: Read>(reader: R) -> &'static str {
    match serde_json::from_reader::<_, CacheVersionProbe>(io::BufReader::new(reader)) {
        Ok(probe) if probe.version == crate::cache::CACHE_VERSION => "ok",
        Ok(_) => "cache-version",
        Err(_) => "corrupt",
    }
}

/// Run every doctor check against `root`, the `--cache` file when given, and the process environment.
#[must_use]
pub fn collect_doctor_report(root: &str, cache: Option<&Path>, no_cache: bool) -> DoctorReport {
    let repo = gix::discover(root).ok();
    let census = grammar_census();
    DoctorReport {
        at: crate::git::history::compute_git_at_stamp(root),
        built_from: env!("CARGO_PKG_VERSION").to_string(),
        checks: vec![
            check_binary_path(),
            check_grammars(&census),
            check_cache_dir(),
            check_git(repo.as_ref()),
            check_tree_sitter(&census),
            check_tracked_binaries(root, repo.as_ref()),
            check_index_cache(root, cache, no_cache),
            check_git_config_trust(repo.as_ref()),
        ],
    }
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

fn check_binary_path() -> DoctorCheck {
    let self_path = std::env::current_exe().ok();
    let name = self_path.as_deref().and_then(Path::file_name).map_or_else(
        || "ripwire-rs".to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let which = find_on_path(&name);
    let same = matches!((&self_path, &which), (Some(a), Some(b)) if same_file(a, b));
    let version = which.as_deref().filter(|_| !same).and_then(version_line);
    binary_path_row(self_path.as_deref(), which.as_deref(), same, version)
}

/// The binary-path row from gathered facts. `version` is the PATH copy's version line and is only
/// printed when that copy is a different file from this binary.
#[must_use]
pub fn binary_path_row(
    self_path: Option<&Path>,
    which: Option<&Path>,
    same: bool,
    version: Option<String>,
) -> DoctorCheck {
    let mut check = DoctorCheck::new("binary-path", which.is_some());
    if let Some(p) = self_path {
        check = check.attr("self", p.display());
    }
    if let Some(p) = which {
        check = check.attr("which", p.display());
    }
    if let (Some(_), false, Some(v)) = (which, same, version) {
        check = check.attr("which_version", v);
    }
    check = check.flag("on_path", which.is_some()).flag("same_file", same);
    match (which, self_path.and_then(Path::parent)) {
        (None, Some(dir)) => check.attr("hint", format!("export PATH=\"{}:$PATH\"", dir.display())),
        _ => check,
    }
}

/// First line of `path --version`; `None` when the binary does not run.
fn version_line(path: &Path) -> Option<String> {
    let out = std::process::Command::new(path).arg("--version").output().ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .map(|line| line.trim().to_string())
}

struct GrammarCensus {
    distinct: usize,
    compatible: usize,
    newest_abi: Option<usize>,
}

fn abi_compatible(abi: usize) -> bool {
    (tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION..=tree_sitter::LANGUAGE_VERSION).contains(&abi)
}

fn grammar_census() -> GrammarCensus {
    let registry = LanguageRegistry;
    let mut seen: HashSet<&tree_sitter::Language> = HashSet::new();
    let mut compatible = 0;
    let mut newest_abi = None;
    for lang in crate::languages::ALL_LANGS {
        if let Some((ts, _)) = registry.get(lang)
            && seen.insert(ts)
        {
            let abi = ts.abi_version();
            compatible += usize::from(abi_compatible(abi));
            newest_abi = newest_abi.max(Some(abi));
        }
    }
    GrammarCensus {
        distinct: seen.len(),
        compatible,
        newest_abi,
    }
}

fn check_grammars(census: &GrammarCensus) -> DoctorCheck {
    DoctorCheck::new(
        "grammars",
        census.distinct > 0 && census.compatible == census.distinct,
    )
    .attr("loaded", census.compatible)
    .attr("expected", census.distinct)
}

fn check_tree_sitter(census: &GrammarCensus) -> DoctorCheck {
    let ok = census.distinct > 0 && census.newest_abi.is_none_or(abi_compatible);
    let mut check =
        DoctorCheck::new("tree-sitter", ok).attr("core_abi", tree_sitter::LANGUAGE_VERSION);
    if let Some(abi) = census.newest_abi {
        check = check.attr("newest_grammar_abi", abi);
    }
    check.attr("languages", crate::languages::ALL_LANGS.len())
}

#[derive(Default)]
struct BlobScan {
    blobs: usize,
    bytes: u64,
    capped: bool,
    truncated: bool,
}

fn scan_cache_blobs(dir: &Path) -> BlobScan {
    let mut scan = BlobScan::default();
    let Ok(entries) = std::fs::read_dir(dir) else {
        scan.truncated = true;
        return scan;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            scan.truncated = true;
            break;
        };
        if !is_cache_blob_name(&entry.file_name().to_string_lossy()) {
            continue;
        }
        if scan.blobs >= BLOB_SCAN_CAP {
            scan.capped = true;
            scan.truncated = true;
            break;
        }
        scan.blobs += 1;
        scan.bytes += entry.metadata().map_or(0, |m| m.len());
    }
    scan
}

fn check_cache_dir() -> DoctorCheck {
    let dir = std::env::temp_dir();
    let writable = std::fs::metadata(&dir).is_ok_and(|m| m.is_dir() && !m.permissions().readonly());
    let scan = scan_cache_blobs(&dir);
    let mut check = DoctorCheck::new("cache-dir", writable)
        .attr("dir", dir.display())
        .attr("blobs", scan.blobs);
    if scan.capped {
        check = check.flag("blobs_floor", true);
    }
    check
        .attr("bytes", scan.bytes)
        .flag("many", scan.blobs > MANY_BLOBS)
        .flag("truncated", scan.truncated)
        .attr("volatile", "blobs,blobs_floor,bytes,many,truncated")
}

fn check_git(repo: Option<&gix::Repository>) -> DoctorCheck {
    let head = repo
        .and_then(|r| r.head_commit().ok())
        .map(|c| c.id().to_string());
    let check = DoctorCheck::new("git", true)
        .flag("git", find_on_path("git").is_some())
        .flag("repo", repo.is_some())
        .flag("history", head.is_some());
    match head {
        Some(sha) => check.attr("head", &sha[..9.min(sha.len())]),
        None => check,
    }
}

fn sniff_binary(path: &Path) -> bool {
    let mut buf = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(SNIFF_BYTES).read_to_end(&mut buf))
        .is_ok_and(|_| crate::crawler::is_binary_buffer(&buf))
}

fn tracked_paths(root: &str, repo: &gix::Repository) -> Option<Vec<PathBuf>> {
    let workdir = repo.workdir()?;
    let prefix = crate::git::repo::workdir_prefix(repo, root)?;
    let index = repo.index_or_empty().ok()?;
    Some(
        index
            .entries()
            .iter()
            .filter_map(|e| {
                gix::path::from_bstr(e.path(&index))
                    .ok()
                    .map(Cow::into_owned)
            })
            .filter(|rel| rel.to_str().is_some_and(|r| r.starts_with(&prefix)))
            .map(|rel| workdir.join(rel))
            .collect(),
    )
}

fn check_tracked_binaries(root: &str, repo: Option<&gix::Repository>) -> DoctorCheck {
    let Some(paths) = repo.and_then(|r| tracked_paths(root, r)) else {
        return DoctorCheck::new("tracked-binaries", true).flag("non_git", true);
    };
    let truncated = paths.len() > TRACKED_SCAN_CAP;
    let check = DoctorCheck::new("tracked-binaries", true).attr("tracked", paths.len());
    let check = if truncated {
        check
    } else {
        check.attr("binaries", paths.iter().filter(|p| sniff_binary(p)).count())
    };
    check.flag("non_git", false).flag("truncated", truncated)
}

/// Which index artifact a run reads: `disabled` under --no-cache, else the `--cache` file, else auto.
#[must_use]
pub fn index_source(no_cache: bool, cache: Option<&Path>) -> &'static str {
    match (no_cache, cache) {
        (true, _) => "disabled",
        (false, Some(_)) => "cache-flag",
        (false, None) => "auto",
    }
}

fn check_index_cache(root: &str, cache: Option<&Path>, no_cache: bool) -> DoctorCheck {
    let (source, path) = index_artifact(cache, no_cache, crate::cache::auto_cache_path(root));
    let state = match std::fs::File::open(&path) {
        Ok(file) => probe_cache_artifact(file),
        Err(e) => open_error_state(e.kind()),
    };
    index_cache_row(source, &path, state)
}

/// The artifact a run reads and its source label: the `--cache` file unless `--no-cache`, else the auto-cache.
#[must_use]
pub fn index_artifact(cache: Option<&Path>, no_cache: bool, auto: PathBuf) -> (&'static str, PathBuf) {
    let path = match cache {
        Some(file) if !no_cache => file.to_path_buf(),
        _ => auto,
    };
    (index_source(no_cache, cache), path)
}

/// Probe state for an artifact that could not be opened.
#[must_use]
pub fn open_error_state(kind: io::ErrorKind) -> &'static str {
    if kind == io::ErrorKind::NotFound {
        "absent"
    } else {
        "unreadable"
    }
}

/// The index-cache row from the chosen artifact and its probed state.
#[must_use]
pub fn index_cache_row(source: &'static str, path: &Path, state: &'static str) -> DoctorCheck {
    DoctorCheck::new("index-cache", !matches!(state, "corrupt" | "unreadable"))
        .attr("cache_version", crate::cache::CACHE_VERSION)
        .attr("source", source)
        .attr("lean_path", path.display())
        .attr("lean", state)
        .attr("volatile", "lean")
}

fn local_filter_drivers(config: &gix::config::File) -> usize {
    config
        .sections_by_name("filter")
        .into_iter()
        .flatten()
        .filter(|s| {
            matches!(
                s.meta().source,
                gix::config::Source::Local | gix::config::Source::Worktree
            )
        })
        .filter_map(|s| s.header().subsection_name().map(|n| n.to_vec()))
        .collect::<BTreeSet<_>>()
        .len()
}

fn check_git_config_trust(repo: Option<&gix::Repository>) -> DoctorCheck {
    let Some(repo) = repo else {
        return DoctorCheck::new("git-config-trust", true).flag("repo", false);
    };
    let snapshot = repo.config_snapshot();
    let raw = snapshot
        .string("core.fsmonitor")
        .map(|v| String::from_utf8_lossy(&v).into_owned());
    let fsmonitor = classify_fsmonitor(raw.as_deref());
    DoctorCheck::new("git-config-trust", true)
        .attr("fsmonitor", fsmonitor)
        .flag("neutralised", fsmonitor == "hook")
        .attr("filter_drivers", local_filter_drivers(snapshot.plumbing()))
}

report_command! {
    name: "doctor",
    args: DoctorArgs,
    static: DOCTOR,
    run: report(render),
    /// Environment and dependency health diagnostics
    #[arg(long)]
    doctor: bool,
}

pub fn render<W: Write>(
    ctx: &ExecutionContext<'_>,
    _: &DoctorArgs,
    out: &mut XmlWriter<'_, W>,
) -> io::Result<()> {
    let cache = ctx.cli.modifiers.cache.as_deref().map(Path::new);
    collect_doctor_report(ctx.root, cache, ctx.cli.modifiers.no_cache).write_xml(out)
}
