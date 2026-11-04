use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const URL_PREFIXES: [&str; 5] = ["https://", "http://", "ssh://", "git://", "git@"];

/// True when a root argument names a remote git repository rather than a local path.
#[must_use]
pub fn is_remote_git_url(root: &str) -> bool {
    URL_PREFIXES.iter().any(|p| root.starts_with(p))
        || (root.ends_with(".git") && root.contains('@') && root.contains(':'))
}

/// Cache directory under `base` a remote root is shallow-cloned into: `<base>/ripwire-rs/remote-<fnv1a64(url)>`.
#[must_use]
pub fn remote_cache_dir(base: &Path, url: &str) -> PathBuf {
    let hash = url.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    base.join("ripwire-rs").join(format!("remote-{hash:016x}"))
}

/// Local checkout of `url`: the cached shallow clone, or a fresh one when absent or `refetch`.
pub fn checkout_remote_root(url: &str, refetch: bool) -> io::Result<PathBuf> {
    let dir = remote_cache_dir(&std::env::temp_dir(), url);
    if !refetch && dir.join(".git").is_dir() {
        let days = cache_age_days(&dir);
        eprintln!(
            "ripwire-rs: reusing cached clone of {url} ({days} days old); pass --refetch to update"
        );
        return Ok(dir);
    }
    shallow_clone(url, &dir)?;
    Ok(dir)
}

fn cache_age_days(dir: &std::path::Path) -> u64 {
    std::fs::metadata(dir)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map_or(0, |d| d.as_secs() / 86_400)
}

fn shallow_clone(url: &str, dir: &std::path::Path) -> io::Result<()> {
    reset_dir(dir)?;
    eprintln!("ripwire-rs: cloning {url} → {}", dir.display());
    let status = git_clone(url, dir)?;
    if status.success() {
        return Ok(());
    }
    let _ = std::fs::remove_dir_all(dir);
    Err(io::Error::other(format!(
        "git clone of {url} failed ({status})"
    )))
}

fn reset_dir(dir: &std::path::Path) -> io::Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    dir.parent().map_or(Ok(()), std::fs::create_dir_all)
}

fn git_clone(url: &str, dir: &std::path::Path) -> io::Result<std::process::ExitStatus> {
    Command::new("git")
        .args([
            "-c",
            "protocol.ext.allow=never",
            "-c",
            "protocol.file.allow=user",
            "-c",
            "core.quotepath=false",
            "-c",
            "core.fsmonitor=false",
            "clone",
            "--depth=1",
            "-q",
            "--",
        ])
        .arg(url)
        .arg(dir)
        .status()
}
