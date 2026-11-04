use crate::domain::{IngestResult, Interner};
use crate::gate::affected::AffectedTest;

/// The suite registry whose text names the shell gates that belong to the test suite.
pub const SCRIPT_GATE_REGISTRY: &str = "test/regression.sh";

/// Shell test gates (`test/*.sh` runners) and the ones the registry names as suite members.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ScriptGates {
    /// Shell runners under a `test/` directory, the registry included.
    pub unmodelled: usize,
    /// Registered gates with the indexed files each names by exact path, in path order.
    pub registered: Vec<(String, Vec<String>)>,
}

impl ScriptGates {
    /// Registered gates with dependency evidence (at least one named indexed file).
    #[must_use]
    pub fn mapped(&self) -> usize {
        self.registered
            .iter()
            .filter(|(_, deps)| !deps.is_empty())
            .count()
    }

    /// Test rows for registered gates that name one of `changed`, skipping paths already listed.
    #[must_use]
    pub fn covering_rows(&self, changed: &[String], listed: &[AffectedTest]) -> Vec<AffectedTest> {
        self.registered
            .iter()
            .filter(|(gate, deps)| {
                deps.iter().any(|d| changed.contains(d)) && !listed.iter().any(|t| &t.path == gate)
            })
            .map(|(gate, _)| AffectedTest {
                script_literal: true,
                ..AffectedTest::new(gate.clone(), 0, false, false)
            })
            .collect()
    }
}

/// Classify `scripts` (`test/*.sh` paths with their text) against the `indexed` file paths.
#[must_use]
pub fn classify_script_gates(scripts: &[(&str, &str)], indexed: &[&str]) -> ScriptGates {
    let registry = scripts
        .iter()
        .find(|(path, _)| *path == SCRIPT_GATE_REGISTRY)
        .map(|(_, text)| *text);
    let registered = registry.map_or_else(Vec::new, |reg| {
        scripts
            .iter()
            .filter(|(path, _)| {
                *path != SCRIPT_GATE_REGISTRY
                    && path
                        .rsplit('/')
                        .next()
                        .is_some_and(|name| contains_token(reg, name))
            })
            .map(|(path, text)| {
                let deps = indexed
                    .iter()
                    .filter(|p| contains_token(text, p))
                    .map(|p| (*p).to_string())
                    .collect();
                ((*path).to_string(), deps)
            })
            .collect()
    });
    ScriptGates {
        unmodelled: scripts.len(),
        registered,
    }
}

/// Read every shell runner under a `test/` directory and classify it against the index.
#[must_use]
pub fn collect_script_gates(root: &str, ingest: &IngestResult, interner: &Interner) -> ScriptGates {
    let paths: Vec<&str> = ingest
        .files
        .iter()
        .map(|f| interner.resolve(&f.path))
        .collect();
    let texts: Vec<(&str, String)> = ingest
        .files
        .iter()
        .filter(|f| f.lang.semantics().is_script_runner())
        .map(|f| interner.resolve(&f.path))
        .filter(|p| {
            p.rsplit_once('/')
                .is_some_and(|(d, _)| d == "test" || d.ends_with("/test"))
        })
        .map(|p| {
            let bytes = crate::crawler::read_rel_bytes(root, p).unwrap_or_default();
            (p, String::from_utf8_lossy(&bytes).into_owned())
        })
        .collect();
    let scripts: Vec<(&str, &str)> = texts.iter().map(|(p, t)| (*p, t.as_str())).collect();
    let mut gates = classify_script_gates(&scripts, &paths);
    gates.registered.sort_unstable();
    gates
}

fn is_path_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')
}

fn contains_token(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    haystack.match_indices(needle).any(|(i, _)| {
        let end = i + needle.len();
        (i == 0 || !is_path_byte(bytes[i - 1])) && (end == bytes.len() || !is_path_byte(bytes[end]))
    })
}
