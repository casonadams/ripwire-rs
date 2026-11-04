use crate::languages::common::util::universal_test_partner_subject;

/// JS-family runner: the nearest manifest's vitest/jest, else `require('node:test')` when `node_test` applies.
#[must_use]
pub(crate) fn js_test_run_command(
    path: &str,
    source: &str,
    manifest: Option<&str>,
    node_test: bool,
) -> Option<String> {
    if is_assertion_script(path) {
        // Run by the command its leading comment block documents, not by the manifest's test runner.
        return assertion_header_command(path, source);
    }
    match manifest {
        Some(m) if m.contains("vitest") => Some(format!("npx vitest run {path}")),
        Some(m) if m.contains("jest") => Some(format!("npx jest {path}")),
        _ => (node_test
            && (source.contains("require('node:test')")
                || source.contains("require(\"node:test\")")))
            .then(|| format!("node --test {path}")),
    }
}

/// `x.check.ts` / `.tsx` / `.js`: a standalone assertion script named after the subject it checks.
#[must_use]
pub(crate) fn is_assertion_script(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    [".check.ts", ".check.tsx", ".check.js"]
        .iter()
        .any(|suffix| name.ends_with(suffix))
}

/// Runner prefixes an assertion script's header may document, longest first.
const HEADER_RUNNERS: [&[&str]; 6] = [
    &["pnpm", "dlx", "tsx"],
    &["npx", "tsx"],
    &["tsx"],
    &["node"],
    &["bun"],
    &["deno", "run"],
];

/// The command an assertion script's leading comment block documents for running it, e.g.
/// `//   pnpm dlx tsx src/x.check.ts`. Trusted only when the named file is this script (repo-relative
/// path or basename); the derived command always names the repo-relative path.
fn assertion_header_command(path: &str, source: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    source
        .lines()
        .map(str::trim)
        .take_while(|line| {
            line.is_empty() || line.starts_with("//") || line.starts_with("/*") || line.starts_with('*')
        })
        .find_map(|line| {
            let words: Vec<&str> = line.trim_start_matches(['/', '*']).split_whitespace().collect();
            (0..words.len()).find_map(|at| header_runner_at(&words[at..], path, name))
        })
}

/// The runner command starting at `words[0]` when it names this script as its file argument.
fn header_runner_at(words: &[&str], path: &str, name: &str) -> Option<String> {
    HEADER_RUNNERS.into_iter().find_map(|runner| {
        let target = words.get(runner.len())?;
        (words.starts_with(runner) && (*target == path || *target == name))
            .then(|| format!("{} {path}", runner.join(" ")))
    })
}

/// Partner subject for JS-family files: an assertion script names its subject as `x.check`, else the
/// universal test-naming conventions apply.
pub(crate) fn js_test_partner_subject(file_stem: &str, in_test_dir: bool) -> Option<&str> {
    file_stem
        .strip_suffix(".check")
        .filter(|subject| !subject.is_empty())
        .or_else(|| universal_test_partner_subject(file_stem, in_test_dir))
}
