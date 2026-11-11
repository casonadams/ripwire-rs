use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/embedded_skills.rs"));

/// Report returned after installing embedded agent skills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillInstallReport {
    pub installed_count: usize,
    pub destination: PathBuf,
    pub skills: Vec<String>,
}

/// Environment inputs to skills-directory resolution, read once so resolution is a pure function.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillsEnv {
    /// `AGENTS_HOME`, when set.
    pub agents_home: Option<String>,
    /// `HOME` (or `USERPROFILE` on Windows), when set.
    pub home: Option<String>,
}

impl SkillsEnv {
    /// Read `AGENTS_HOME` and the home directory from the process environment.
    #[must_use]
    pub fn from_process() -> Self {
        let home = std::env::var("HOME").ok().or_else(|| {
            if cfg!(windows) {
                std::env::var("USERPROFILE").ok()
            } else {
                None
            }
        });
        Self::from_values(std::env::var("AGENTS_HOME").ok(), home)
    }

    /// An environment holding only the absolute values: an empty or relative root is ignored, as the XDG
    /// Base Directory spec directs, so it can never place files relative to the working directory.
    #[must_use]
    pub fn from_values(agents_home: Option<String>, home: Option<String>) -> Self {
        let absolute = |value: Option<String>| value.filter(|p| Path::new(p).is_absolute());
        Self {
            agents_home: absolute(agents_home),
            home: absolute(home),
        }
    }
}

/// Compute default cross-agent skills directory (~/.agents/skills).
pub fn default_skills_dir() -> PathBuf {
    default_skills_dir_in(&SkillsEnv::from_process())
}

/// [`default_skills_dir`] for the given environment.
#[must_use]
pub fn default_skills_dir_in(env: &SkillsEnv) -> PathBuf {
    if let Some(agents_home) = &env.agents_home {
        return PathBuf::from(agents_home).join("skills");
    }
    if let Some(home) = &env.home {
        return PathBuf::from(home).join(".agents").join("skills");
    }
    PathBuf::from(".agents").join("skills")
}

/// Resolve destination skills directory from optional user CLI argument.
pub fn resolve_skills_dir(custom_path: Option<&str>) -> PathBuf {
    resolve_skills_dir_in(custom_path, &SkillsEnv::from_process())
}

/// [`resolve_skills_dir`] for the given environment.
#[must_use]
pub fn resolve_skills_dir_in(custom_path: Option<&str>, env: &SkillsEnv) -> PathBuf {
    let Some(raw) = custom_path.map(str::trim).filter(|s| !s.is_empty()) else {
        return default_skills_dir_in(env);
    };
    let path = PathBuf::from(expand_tilde_in(raw, env.home.as_deref()));
    if path.file_name().and_then(|n| n.to_str()) == Some(".agents") {
        path.join("skills")
    } else {
        path
    }
}

/// Expand a leading `~` or `~/` to `home`. Other paths, and every path when `home` is `None`, are
/// returned unchanged.
#[must_use]
pub fn expand_tilde_in(path_str: &str, home: Option<&str>) -> String {
    let Some(home) = home else {
        return path_str.to_string();
    };
    if path_str == "~" {
        return home.to_string();
    }
    match path_str.strip_prefix("~/") {
        Some(rest) => format!("{home}/{rest}"),
        None => path_str.to_string(),
    }
}

/// Extract unique skill directory names from embedded assets.
pub fn get_skill_names() -> Vec<&'static str> {
    let mut names = BTreeSet::new();
    for (rel_path, _) in EMBEDDED_SKILLS {
        if let Some(first_seg) = rel_path.split('/').next()
            && first_seg.starts_with("ripwire-rs-")
        {
            names.insert(first_seg);
        }
    }
    names.into_iter().collect()
}

/// Generate `.ripwire-rs-manifest-v1` format content.
pub fn generate_manifest_content(skills: &[&str]) -> String {
    let mut out = String::from("version=1\n");
    for skill in skills {
        out.push_str("skill=");
        out.push_str(skill);
        out.push('\n');
    }
    out
}

fn clean_path_if_symlink(path: &Path) -> io::Result<()> {
    if let Ok(meta) = fs::symlink_metadata(path)
        && meta.file_type().is_symlink()
    {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn write_skill_file(dest_dir: &Path, rel_path: &str, content: &str) -> io::Result<()> {
    let target_file = dest_dir.join(rel_path);
    if let Some(parent) = target_file.parent() {
        clean_path_if_symlink(parent)?;
        fs::create_dir_all(parent)?;
    }
    clean_path_if_symlink(&target_file)?;
    fs::write(&target_file, content)
}
fn write_manifest_file(dest_dir: &Path, skill_names: &[&str]) -> io::Result<()> {
    let manifest_content = generate_manifest_content(skill_names);
    let manifest_path = dest_dir.join(".ripwire-rs-manifest-v1");
    clean_path_if_symlink(&manifest_path)?;
    fs::write(&manifest_path, manifest_content)
}

fn write_all_skill_files(dest_dir: &Path) -> io::Result<()> {
    for (rel_path, content) in EMBEDDED_SKILLS {
        write_skill_file(dest_dir, rel_path, content)?;
    }
    Ok(())
}

/// Where an install writes: the explicit `custom_path`, else the default skills directory, which must be
/// absolute so a missing or unusable `HOME` and `AGENTS_HOME` cannot write into the working directory.
pub fn install_destination(custom_path: Option<&str>, env: &SkillsEnv) -> io::Result<PathBuf> {
    let dest = resolve_skills_dir_in(custom_path, env);
    let explicit = custom_path.is_some_and(|p| !p.trim().is_empty());
    if !explicit && !dest.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "no absolute skills directory: set HOME or AGENTS_HOME to an absolute path, or pass a DIR",
        ));
    }
    Ok(dest)
}

/// Install embedded skills and write active manifest to destination directory.
pub fn install_skills(custom_path: Option<&str>) -> io::Result<SkillInstallReport> {
    let dest_dir = install_destination(custom_path, &SkillsEnv::from_process())?;
    fs::create_dir_all(&dest_dir)?;
    write_all_skill_files(&dest_dir)?;
    let skill_names = get_skill_names();
    write_manifest_file(&dest_dir, &skill_names)?;

    Ok(SkillInstallReport {
        installed_count: skill_names.len(),
        destination: dest_dir,
        skills: skill_names.into_iter().map(String::from).collect(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillScanFinding {
    pub path: String,
    pub line: usize,
    pub rule: &'static str,
    pub sev: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillScanReport {
    pub files_scanned: usize,
    pub findings: Vec<SkillScanFinding>,
    pub verdict: &'static str,
}

fn strip_balanced_delim(line: &str, delim: char) -> String {
    let count = line.chars().filter(|&c| c == delim).count();
    if count < 2 || !count.is_multiple_of(2) {
        return line.to_string();
    }
    let mut out = String::with_capacity(line.len());
    let mut inside = false;
    for c in line.chars() {
        if c == delim {
            inside = !inside;
            out.push(' ');
        } else if !inside {
            out.push(c);
        }
    }
    out
}

fn check_prose_injection_line(
    path_str: &str,
    line_num: usize,
    line: &str,
    findings: &mut Vec<SkillScanFinding>,
) -> (bool, String) {
    let s1 = strip_balanced_delim(line, '`');
    let s2 = strip_balanced_delim(&s1, '"');
    let lower = s2.to_ascii_lowercase();
    let mut hit = false;
    if lower.contains("ignore previous instructions") {
        findings.push(SkillScanFinding {
            path: path_str.to_string(),
            line: line_num,
            rule: "INJECTION:ignore-prev",
            sev: "critical",
        });
        hit = true;
    }
    if lower.contains("you are now an ") || lower.contains("you are now a ") {
        findings.push(SkillScanFinding {
            path: path_str.to_string(),
            line: line_num,
            rule: "INJECTION:persona-override",
            sev: "critical",
        });
        hit = true;
    }
    if lower.contains("forget everything above") {
        findings.push(SkillScanFinding {
            path: path_str.to_string(),
            line: line_num,
            rule: "INJECTION:forget-context",
            sev: "critical",
        });
        hit = true;
    }
    (hit, s2)
}

fn is_exfil_command_line(l: &str) -> bool {
    if l.starts_with("You could use ") {
        return false;
    }
    let is_loopback = l.contains("127.0.0.1") || l.contains("localhost");
    if is_loopback {
        return false;
    }
    let has_net = l.contains("curl ") || l.contains("wget ");
    let has_target = l.contains('$') || l.contains(" | bash") || l.contains(" | sh");
    let has_pipe_nc = l.contains("| nc ") || l.contains("| netcat ");
    (has_net && has_target) || has_pipe_nc
}

fn update_doc_fence_state(l: &str, in_labeled_doc_fence: &mut bool) -> bool {
    let Some(rest) = l.strip_prefix("```") else {
        return false;
    };
    let tag = rest.trim();
    if matches!(tag, "text" | "xml" | "markdown") {
        *in_labeled_doc_fence = true;
    } else if tag.is_empty() && *in_labeled_doc_fence {
        *in_labeled_doc_fence = false;
    }
    true
}

fn record_exfil_line(
    path_str: &str,
    line_num: usize,
    l: &str,
    findings: &mut Vec<SkillScanFinding>,
) {
    if is_exfil_command_line(l) {
        findings.push(SkillScanFinding {
            path: path_str.to_string(),
            line: line_num,
            rule: "EXFILTRATE:net-exfil",
            sev: "critical",
        });
    }
}

const CREDENTIAL_PATHS: &[&str] = &["$HOME/.ssh", "~/.ssh", "~/.aws"];

fn record_credential_line(
    path_str: &str,
    line_num: usize,
    l: &str,
    findings: &mut Vec<SkillScanFinding>,
) {
    if CREDENTIAL_PATHS.iter().any(|p| l.contains(p)) {
        findings.push(SkillScanFinding {
            path: path_str.to_string(),
            line: line_num,
            rule: "EXFILTRATE:ssh-aws-creds",
            sev: "critical",
        });
    }
}

fn record_split_line_injection(
    path_str: &str,
    per_line_ignore: bool,
    first_ignore_line: usize,
    joined_prose: &str,
    findings: &mut Vec<SkillScanFinding>,
) {
    if !per_line_ignore
        && joined_prose
            .to_ascii_lowercase()
            .contains("ignore previous instructions")
    {
        findings.push(SkillScanFinding {
            path: path_str.to_string(),
            line: first_ignore_line,
            rule: "INJECTION:ignore-prev",
            sev: "critical",
        });
    }
}

fn scan_skill_markdown(path_str: &str, content: &str, findings: &mut Vec<SkillScanFinding>) {
    let mut in_labeled_doc_fence = false;
    let mut joined_prose = String::new();
    let mut first_ignore_line = 1usize;
    let mut per_line_ignore = false;
    for (idx, line) in content.lines().enumerate() {
        let l = line.trim();
        if !update_doc_fence_state(l, &mut in_labeled_doc_fence) && !in_labeled_doc_fence {
            let (hit, stripped) = check_prose_injection_line(path_str, idx + 1, l, findings);
            if stripped.to_ascii_lowercase().contains("ignore previous") {
                first_ignore_line = idx + 1;
            }
            per_line_ignore |= hit;
            joined_prose.push_str(&stripped);
            joined_prose.push(' ');
        }
    }
    record_split_line_injection(
        path_str,
        per_line_ignore,
        first_ignore_line,
        &joined_prose,
        findings,
    );
}

pub fn scan_content_for_exfil(path_str: &str, content: &str, findings: &mut Vec<SkillScanFinding>) {
    for (idx, line) in content.lines().enumerate() {
        let l = line.trim();
        record_exfil_line(path_str, idx + 1, l, findings);
        let prose = strip_balanced_delim(l, '`');
        record_credential_line(path_str, idx + 1, &prose, findings);
    }
    scan_skill_markdown(path_str, content, findings);
}

fn scan_file_for_exfil(path: &Path, findings: &mut Vec<SkillScanFinding>) {
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };
    scan_content_for_exfil(&path.display().to_string(), &content, findings);
}

fn is_scannable_skill_file(p: &Path) -> bool {
    if !p.is_file() {
        return false;
    }
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(ext, "md" | "sh" | "bash" | "zsh" | "ksh")
        || p.file_name().and_then(|n| n.to_str()) == Some("SKILL.md")
}

fn scan_skill_tree(dir: &Path, findings: &mut Vec<SkillScanFinding>) -> usize {
    if dir.is_file() {
        scan_file_for_exfil(dir, findings);
        return 1;
    }
    if !dir.exists() {
        return 0;
    }
    let mut count = 0usize;
    for entry in ignore::WalkBuilder::new(dir)
        .sort_by_file_path(Ord::cmp)
        .build()
        .flatten()
    {
        if is_scannable_skill_file(entry.path()) {
            count += 1;
            scan_file_for_exfil(entry.path(), findings);
        }
    }
    count
}

pub fn scan_skills_directory(dir: &Path) -> io::Result<SkillScanReport> {
    let mut findings = Vec::new();
    let files_scanned = scan_skill_tree(dir, &mut findings);
    let verdict = if findings.iter().any(|f| f.sev == "critical") {
        "critical"
    } else if !findings.is_empty() {
        "warn"
    } else {
        "clean"
    };

    Ok(SkillScanReport {
        files_scanned,
        findings,
        verdict,
    })
}

impl crate::serialize::XmlReport for SkillScanReport {
    fn schema_tag(&self) -> &'static str {
        "skillscan"
    }
    fn schema_version(&self) -> &'static str {
        "ripwire-rs.scan-skills/v1"
    }
    fn include_root_attr(&self) -> bool {
        false
    }
    fn write_extra_attrs<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write!(
            out,
            " files=\"{}\" findings=\"{}\" verdict=\"{}\"",
            self.files_scanned,
            self.findings.len(),
            self.verdict
        )
    }
    fn write_body<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        for f in &self.findings {
            use crate::serialize::XmlEscapeExt;
            writeln!(
                out,
                "  <f p=\"{}:{}\" rule=\"{}\" sev=\"{}\"/>",
                f.path.xml_escaped(),
                f.line,
                f.rule,
                f.sev
            )?;
        }
        Ok(())
    }
}

pub fn serialize_skill_scan_xml<W: io::Write>(
    rep: &SkillScanReport,
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlReport;
    rep.render_envelope(out)
}
