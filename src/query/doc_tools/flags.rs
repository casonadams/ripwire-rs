use crate::domain::{IngestResult, Interner};
use crate::languages::common::flags::FlagFact;
use crate::query::SourceReader;
use serde_json::{Value, json};

#[derive(Default)]
struct GateEntry {
    name: String,
    kind: &'static str,
    default_val: String,
    is_dark: bool,
    file: String,
    line: usize,
    regions: usize,
    loc: usize,
    reads: Vec<(String, usize)>,
}

impl GateEntry {
    fn configure(&mut self, def_val: String, is_dark: bool) {
        if self.reads.len() == 1 {
            self.default_val = def_val;
            self.is_dark = is_dark;
        }
    }
}

struct SettingsGate {
    name: String,
    default: String,
    dark: bool,
    define_line: usize,
}

#[derive(Default)]
struct FlagsScanTally {
    by_key: std::collections::BTreeMap<(String, &'static str), GateEntry>,
    settings: Vec<(SettingsGate, String)>,
    conditional: std::collections::HashMap<String, Vec<(String, usize, usize)>>,
}

impl FlagsScanTally {
    fn record_site(
        &mut self,
        kind: &'static str,
        name: String,
        path: &str,
        line: usize,
    ) -> &mut GateEntry {
        let entry = self
            .by_key
            .entry((name.clone(), kind))
            .or_insert_with(|| GateEntry {
                name,
                kind,
                default_val: "unset".to_string(),
                is_dark: true,
                file: path.to_string(),
                line,
                regions: 0,
                loc: 0,
                reads: Vec::new(),
            });
        entry.reads.push((path.to_string(), line));
        entry
    }

    /// A settings gate counts only where some `#if`-family condition uses it; those uses are its reads.
    fn resolve_settings_gates(&mut self) {
        for (gate, path) in std::mem::take(&mut self.settings) {
            let key = (gate.name.clone(), "compile");
            let Some(uses) = self.conditional.get(&gate.name) else {
                continue;
            };
            if self.by_key.contains_key(&key) {
                continue;
            }
            let mut reads: Vec<(String, usize)> =
                uses.iter().map(|(p, l, _)| (p.clone(), *l)).collect();
            reads.dedup();
            self.by_key.insert(
                key,
                GateEntry {
                    name: gate.name,
                    kind: "compile",
                    default_val: gate.default,
                    is_dark: gate.dark,
                    file: path,
                    line: gate.define_line + 1,
                    regions: uses.len(),
                    loc: uses.iter().map(|(_, _, loc)| loc).sum(),
                    reads,
                },
            );
        }
    }
}

fn scan_file_flags(
    sources: &(impl SourceReader + ?Sized),
    path: &str,
    lang: crate::domain::Lang,
    tally: &mut FlagsScanTally,
) {
    let Ok(bytes) = sources.read(path) else {
        return;
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return;
    };
    let lines: Vec<&str> = text.lines().collect();
    let literal = literal_block_lines(&lines, lang);
    for fact in lang.semantics().flag_facts(&lines, path) {
        match fact {
            FlagFact::Conditional { name, line, loc } => {
                tally
                    .conditional
                    .entry(name)
                    .or_default()
                    .push((path.to_string(), line + 1, loc));
            }
            FlagFact::Env { line, .. }
            | FlagFact::Option { line, .. }
            | FlagFact::Setting { line, .. }
                if literal[line] => {}
            FlagFact::Env { name, line } => {
                tally.record_site("env", name, path, line + 1);
            }
            FlagFact::Option {
                kind,
                name,
                default,
                dark,
                line,
            } => tally
                .record_site(kind, name, path, line + 1)
                .configure(default, dark),
            FlagFact::Setting {
                name,
                default,
                dark,
                define_line,
                ..
            } => tally.settings.push((
                SettingsGate {
                    name,
                    default,
                    dark,
                    define_line,
                },
                path.to_string(),
            )),
        }
    }
}

fn literal_block_lines(lines: &[&str], lang: crate::domain::Lang) -> Vec<bool> {
    let mut literal = vec![false; lines.len()];
    let mut until: Option<&str> = None;
    for (idx, line) in lines.iter().enumerate() {
        match until {
            Some(term) => {
                literal[idx] = true;
                if line.trim() == term {
                    until = None;
                }
            }
            None => until = lang.semantics().quoted_heredoc_terminator(line),
        }
    }
    literal
}

fn scan_flag_manifests(sources: &(impl SourceReader + ?Sized), tally: &mut FlagsScanTally) {
    for lang in crate::languages::ALL_LANGS {
        for manifest in lang.semantics().flag_manifests() {
            scan_file_flags(sources, manifest, lang, tally);
        }
    }
}

pub fn flags_from_sources(
    root: &str,
    sources: &(impl SourceReader + ?Sized),
    ingest: &IngestResult,
    interner: &Interner,
) -> Value {
    let mut tally = FlagsScanTally::default();

    for file in &ingest.files {
        if !file.lang.semantics().is_prose_document() {
            let path = interner.resolve(&file.path);
            scan_file_flags(sources, path, file.lang, &mut tally);
        }
    }
    scan_flag_manifests(sources, &mut tally);
    tally.resolve_settings_gates();

    let mut flags = Vec::with_capacity(tally.by_key.len());
    let mut compile_count = 0usize;
    let mut cmake_count = 0usize;
    let mut env_count = 0usize;
    let mut dark_gates = 0usize;
    for entry in tally.by_key.into_values() {
        match entry.kind {
            "compile" => compile_count += 1,
            "cmake" => cmake_count += 1,
            _ => env_count += 1,
        }
        dark_gates += usize::from(entry.is_dark);
        let reads_json: Vec<Value> = entry
            .reads
            .iter()
            .map(|(p, l)| json!({ "path": p, "line": l }))
            .collect();
        flags.push(json!({
            "name": entry.name,
            "flag": entry.name,
            "kind": entry.kind,
            "default": entry.default_val,
            "dark": usize::from(entry.is_dark),
            "regions": entry.regions,
            "loc": entry.loc,
            "reads_count": entry.reads.len(),
            "reads": reads_json,
            "file": entry.file,
            "path": entry.file,
            "line": entry.line
        }));
    }

    json!({
        "root": root,
        "files": ingest.files.len(),
        "flags_count": flags.len(),
        "compile_count": compile_count,
        "cmake_count": cmake_count,
        "env_count": env_count,
        "dark_gates": dark_gates,
        "gates": flags.clone(),
        "flags": flags
    })
}
