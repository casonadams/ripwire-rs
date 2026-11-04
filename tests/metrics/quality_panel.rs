use ripwire_rs::domain::Binding;
use ripwire_rs::git::history::CommitInfo;
use ripwire_rs::metrics::{
    PanelHistory, PanelInputs, QualityPanelReport, compute_quality_panel_report,
};
use ripwire_rs::serialize::PageRequest;
use ripwire_rs::{
    FileInfo, Graph, IngestResult, Interner, Lang, RefRole, Reference, SymKind, Symbol, ToXmlString,
};

const NOW: i64 = 1_700_000_000;
const WINDOW_START: i64 = NOW - 365 * 86_400;
const PATH: &str = "src/lib.rs";
const NO_HISTORY: &str = "in-memory ingest: no git history";

fn function(interner: &Interner, id: u32, name: &str) -> Symbol {
    function_in(interner, id, 0, PATH, name)
}

fn ingest_for(interner: &Interner, symbols: Vec<Symbol>) -> IngestResult {
    ingest_files(interner, &[PATH], symbols)
}

fn commits_in_window(count: usize) -> Vec<CommitInfo> {
    commits_touching(PATH, count)
}

/// `count` commits touching `path`, newest first, one day apart, all inside the window.
fn commits_touching(path: &str, count: usize) -> Vec<CommitInfo> {
    (0..count)
        .map(|i| CommitInfo {
            id: format!("{path}{i}"),
            timestamp: NOW - i as i64 * 86_400,
            author: "Ada".to_string(),
            changed_files: vec![path.to_string()],
        })
        .collect()
}

/// A function in file `file_id` (whose path is `path`) of a multi-file in-memory ingest.
fn function_in(interner: &Interner, id: u32, file_id: u32, path: &str, name: &str) -> Symbol {
    Symbol::new(
        id,
        interner.intern(name),
        interner.intern(&format!("{path}::{name}")),
        SymKind::Function,
        Lang::Rust,
        file_id,
        id + 1,
    )
}

fn ingest_files(interner: &Interner, paths: &[&str], symbols: Vec<Symbol>) -> IngestResult {
    let mut ingest = IngestResult::new();
    for (id, path) in paths.iter().enumerate() {
        ingest.files.push(FileInfo {
            id: id as u32,
            path: interner.intern(path),
            lang: Lang::Rust,
            line_count: 150,
            byte_size: 3000,
        });
    }
    ingest.symbols = symbols;
    ingest
}

fn history(
    ingest: &IngestResult,
    interner: &Interner,
    commits: &[CommitInfo],
) -> Result<PanelHistory, String> {
    Ok(PanelHistory::from_commits(
        ingest,
        interner,
        commits,
        "abc123def".to_string(),
        WINDOW_START,
    ))
}

fn panel(
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    preset: Option<&str>,
    history: Result<PanelHistory, String>,
) -> QualityPanelReport {
    compute_quality_panel_report(
        ingest,
        graph,
        interner,
        PanelInputs { preset, history },
        PageRequest::default(),
    )
}

fn family_attrs(rep: &QualityPanelReport, name: &str, family: &str) -> String {
    rep.rows
        .iter()
        .find(|r| r.name == name)
        .expect("ranked row")
        .evidence
        .iter()
        .find(|e| e.family == family)
        .unwrap_or_else(|| panic!("{family} evidence for {name}"))
        .attrs
        .clone()
}

fn four_files_panel(counts: [usize; 4], name: impl Fn(usize) -> String) -> QualityPanelReport {
    let interner = Interner::new();
    let paths = ["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs"];
    let symbols = paths
        .iter()
        .enumerate()
        .map(|(i, p)| function_in(&interner, i as u32, i as u32, p, &name(i)))
        .collect();
    let ingest = ingest_files(&interner, &paths, symbols);
    let graph = Graph::build(&ingest, &interner);
    let commits: Vec<CommitInfo> = paths
        .iter()
        .zip(counts)
        .flat_map(|(p, n)| commits_touching(p, n))
        .collect();
    panel(
        &ingest,
        &graph,
        &interner,
        Some("lenient"),
        history(&ingest, &interner, &commits),
    )
}

#[test]
fn test_quality_panel_computation() {
    let ingest = IngestResult::new();
    let interner = Interner::new();
    let graph = Graph::build(&ingest, &interner);
    let rep = panel(
        &ingest,
        &graph,
        &interner,
        None,
        Err(NO_HISTORY.to_string()),
    );
    assert_eq!(rep.preset, "default");
    assert_eq!(rep.cut, 2);
    assert!(
        rep.to_xml_string()
            .contains("schema=\"ripwire-rs.quality-panel/v1\"")
    );
}

#[test]
fn test_check_symbol_families() {
    let interner = Interner::new();
    let mut sym = function(&interner, 0, "complex_fn");
    sym.ccx = 20;
    sym.loc = 100;
    sym.max_nest = 5;
    sym.params = 6;
    sym.cx = 15;
    let state_sym = Symbol::new(
        1,
        interner.intern("GLOBAL_STATE"),
        interner.intern("src/lib.rs::GLOBAL_STATE"),
        SymKind::Var,
        Lang::Rust,
        0,
        120,
    );
    let mut ingest = ingest_for(&interner, vec![sym, state_sym]);
    ingest.references.push(Reference::new(
        0,
        interner.intern("GLOBAL_STATE"),
        0,
        10,
        Lang::Rust,
        RefRole::Write,
    ));
    let graph = Graph::build(&ingest, &interner);
    let commits = commits_in_window(14);
    let rep = panel(
        &ingest,
        &graph,
        &interner,
        None,
        history(&ingest, &interner, &commits),
    );
    assert_eq!(rep.rows.len(), 1);
    assert!(rep.rows[0].fam >= 3);
    assert!(rep.rows[0].fired.contains("structural"));
    assert!(rep.rows[0].fired.contains("state"));
    assert!(rep.rows[0].fired.contains("historical"));
}

#[test]
fn test_historical_follows_churn_not_structure() {
    let interner = Interner::new();
    let simple = function(&interner, 0, "handle_request");
    let mut tangled = function(&interner, 1, "tangled_dispatch");
    tangled.ccx = 30;
    tangled.loc = 100;
    tangled.max_nest = 5;
    let ingest = ingest_for(&interner, vec![simple, tangled]);
    let graph = Graph::build(&ingest, &interner);
    let run = |commits: &[CommitInfo]| {
        panel(
            &ingest,
            &graph,
            &interner,
            Some("lenient"),
            history(&ingest, &interner, commits),
        )
    };

    let busy = run(&commits_in_window(14));
    assert_eq!(busy.hranked, 2);

    let quiet = run(&commits_in_window(2));
    assert_eq!(quiet.hranked, 0);
    assert_eq!(quiet.rows.len(), 1);
    assert_eq!(quiet.rows[0].name, "tangled_dispatch");
    assert_eq!(quiet.rows[0].fired, "structural");

    let floor = run(&commits_in_window(3));
    assert_eq!(floor.hranked, 2, "a file at the floor fires");

    let stale: Vec<CommitInfo> = (0..14)
        .map(|i| CommitInfo {
            id: format!("old{i}"),
            timestamp: WINDOW_START - 1 - i * 86_400,
            author: "Ada".to_string(),
            changed_files: vec![PATH.to_string()],
        })
        .collect();
    assert_eq!(
        run(&stale).hranked,
        0,
        "commits before the window must not count"
    );
}

#[test]
fn test_lexical_family_detects_weak_names_that_parameter_count_cannot() {
    let interner = Interner::new();
    let mut terse = function(&interner, 0, "f");
    terse.params = 0;
    terse.ccx = 1;
    terse.loc = 5;
    let mut many_params = function(&interner, 1, "fetch_order");
    many_params.params = 9;
    many_params.ccx = 1;
    many_params.loc = 5;
    let rep = lenient_panel(&interner, vec![terse, many_params]);
    let terse_row = rep.rows.iter().find(|r| r.name == "f").expect("terse row");
    assert_eq!(terse_row.fired, "lexical");
    assert!(terse_row.fired.contains("lexical"));
    assert!(family_attrs(&rep, "f", "lexical").contains("name_len=\"1\""));

    let long_row = rep
        .rows
        .iter()
        .find(|r| r.name == "fetch_order")
        .expect("fetch_order row");
    assert!(!long_row.fired.contains("lexical"));
    assert!(long_row.fired.contains("structural"));

    let mut deep = function(&interner, 0, "deep_dispatch");
    deep.ccx = 30;
    deep.loc = 100;
    deep.max_nest = 5;
    deep.humps = 3;
    deep.deep = 39;
    let mut flat = function(&interner, 0, "flat_dispatch");
    flat.ccx = 30;
    flat.loc = 100;
    flat.max_nest = 5;
    let rep = lenient_panel(&interner, vec![deep, flat]);
    let informative = family_attrs(&rep, "deep_dispatch", "structural");
    assert!(informative.contains("humps=\"3\""));
    assert!(informative.contains("deep=\"39\""));
    let quiet = family_attrs(&rep, "flat_dispatch", "structural");
    assert!(!quiet.contains("humps="));
    assert!(!quiet.contains("deep="));
    assert!(quiet.contains("ccx=\"30\""));
}

fn lenient_panel(interner: &Interner, symbols: Vec<Symbol>) -> QualityPanelReport {
    let ingest = ingest_for(interner, symbols);
    let graph = Graph::build(&ingest, interner);
    panel(
        &ingest,
        &graph,
        interner,
        Some("lenient"),
        Err(NO_HISTORY.to_string()),
    )
}

#[test]
fn test_unavailable_history_is_disclosed_not_substituted() {
    let interner = Interner::new();
    let mut tangled = function(&interner, 0, "tangled_dispatch");
    tangled.ccx = 30;
    tangled.loc = 100;
    tangled.max_nest = 5;
    let ingest = ingest_for(&interner, vec![tangled]);
    let graph = Graph::build(&ingest, &interner);
    let rep = panel(
        &ingest,
        &graph,
        &interner,
        Some("lenient"),
        Err("not a git repository".to_string()),
    );
    assert!(!rep.enabled.contains(&"historical"));
    assert!(
        rep.unavailable
            .iter()
            .any(|(family, why)| *family == "historical" && why.contains("not a git repository"))
    );
    let row = &rep.rows[0];
    assert!(row.evidence.iter().all(|e| e.family != "historical"));
    let xml = rep.to_xml_string();
    assert!(xml.contains("unavailable=\"confusion,historical\""));
    assert!(xml.contains("hstatus=\"unavailable\""));
    assert!(!xml.contains(" at=\""));
    assert!(!xml.contains(" window=\""));
}

#[test]
fn test_denominators_and_evidence_match_enabled_families() {
    let interner = Interner::new();
    let mut tangled = function(&interner, 0, "tangled_dispatch");
    tangled.ccx = 30;
    tangled.loc = 100;
    tangled.max_nest = 5;
    tangled.params = 6;
    let ingest = ingest_for(&interner, vec![tangled]);
    let graph = Graph::build(&ingest, &interner);
    let commits = commits_in_window(14);
    let rep = panel(
        &ingest,
        &graph,
        &interner,
        Some("lenient"),
        history(&ingest, &interner, &commits),
    );
    // Rust has no C-layout confusion family: structural, lexical, historical, colocation, state.
    assert_eq!(rep.enabled.len(), 5);
    let row = &rep.rows[0];
    assert_eq!(row.of, rep.enabled.len());
    assert_eq!(row.evidence.len(), row.fam);

    let structural = row
        .evidence
        .iter()
        .find(|e| e.family == "structural")
        .expect("structural evidence");
    assert!(row.fired.contains("structural"));
    assert!(structural.attrs.contains("ccx=\"30\""));
    assert!(structural.attrs.contains("loc=\"100\""));
    assert!(structural.attrs.contains("nest=\"5\""));

    let historical = row
        .evidence
        .iter()
        .find(|e| e.family == "historical")
        .expect("historical evidence");
    assert!(row.fired.contains("historical"));
    assert!(historical.attrs.contains("commits=\"14\""));
    assert!(historical.attrs.contains("last=\"2023-11-14\""));

    let xml = rep.to_xml_string();
    assert!(xml.contains("families=\"6\""));
    assert!(xml.contains("enabled_n=\"5\""));
    assert!(xml.contains("at=\"abc123def\""));
}

#[test]
fn test_historical_fires_only_in_the_hottest_file_of_a_quiet_repo() {
    let interner = Interner::new();
    let paths = ["src/hot.rs", "src/warm.rs", "src/cold.rs"];
    let symbols = vec![
        function_in(&interner, 0, 0, paths[0], "hot_a"),
        function_in(&interner, 1, 0, paths[0], "hot_b"),
        function_in(&interner, 2, 1, paths[1], "warm_fn"),
        function_in(&interner, 3, 2, paths[2], "cold_fn"),
    ];
    let ingest = ingest_files(&interner, &paths, symbols);
    let graph = Graph::build(&ingest, &interner);
    let mut commits = commits_touching(paths[0], 5);
    commits.extend(commits_touching(paths[1], 2));
    commits.extend(commits_touching(paths[2], 1));
    let rep = panel(
        &ingest,
        &graph,
        &interner,
        Some("lenient"),
        history(&ingest, &interner, &commits),
    );
    assert_eq!(rep.hranked, 2);
    let historical: Vec<&str> = rep
        .rows
        .iter()
        .filter(|r| r.fired.contains("historical"))
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(historical, ["hot_a", "hot_b"]);
    let xml = rep.to_xml_string();
    assert!(
        xml.contains("hcut=\"5\" hbasis=\"p90\" hfiles=\"3\" hcrossed=\"1\" hstatus=\"ranked\"")
    );
}

#[test]
fn test_uniform_low_churn_below_the_floor_is_measured_and_empty() {
    let rep = four_files_panel([2, 2, 2, 2], |_| "touched_twice".to_string());
    assert_eq!(rep.hranked, 0);
    assert!(rep.rows.iter().all(|r| !r.fired.contains("historical")));
    let xml = rep.to_xml_string();
    assert!(xml.contains(
        "hcut=\"3\" hbasis=\"floor\" hfiles=\"4\" hcrossed=\"0\" hstatus=\"measured-empty\""
    ));
}

#[test]
fn test_busy_repo_keeps_the_fixed_ceiling() {
    let rep = four_files_panel([20, 20, 13, 14], |i| format!("f{i}"));
    let historical: Vec<&str> = rep
        .rows
        .iter()
        .filter(|r| r.fired.contains("historical"))
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(
        historical,
        ["f0", "f1", "f3"],
        "13 commits stays below the ceiling"
    );
    let xml = rep.to_xml_string();
    assert!(
        xml.contains(
            "hcut=\"14\" hbasis=\"ceiling\" hfiles=\"4\" hcrossed=\"3\" hstatus=\"ranked\""
        )
    );
}

#[test]
fn test_state_means_mutable_state_not_configuration_reads() {
    let interner = Interner::new();
    let writer = function(&interner, 0, "bump_counter");
    let mut reader = function(&interner, 1, "read_limits");
    reader.params = 6;
    let counter = Symbol::new(
        2,
        interner.intern("COUNTER"),
        interner.intern("src/lib.rs::COUNTER"),
        SymKind::Var,
        Lang::Rust,
        0,
        120,
    );
    let mut limits = Symbol::new(
        3,
        interner.intern("LIMITS"),
        interner.intern("src/lib.rs::LIMITS"),
        SymKind::Var,
        Lang::Rust,
        0,
        121,
    );
    limits.binding = Binding::Const;
    let mut ingest = ingest_for(&interner, vec![writer, reader, counter, limits]);
    ingest.references.push(Reference::new(
        0,
        interner.intern("COUNTER"),
        0,
        10,
        Lang::Rust,
        RefRole::Write,
    ));
    ingest.references.push(Reference::new(
        1,
        interner.intern("LIMITS"),
        0,
        20,
        Lang::Rust,
        RefRole::Read,
    ));
    let graph = Graph::build(&ingest, &interner);
    let rep = panel(
        &ingest,
        &graph,
        &interner,
        Some("lenient"),
        Err(NO_HISTORY.to_string()),
    );
    let writer_row = rep
        .rows
        .iter()
        .find(|r| r.name == "bump_counter")
        .expect("writer row");
    assert!(writer_row.fired.contains("state"));
    let state = writer_row
        .evidence
        .iter()
        .find(|e| e.family == "state")
        .expect("state evidence");
    assert!(state.attrs.contains("writes=\"1\""));
    let reader_row = rep
        .rows
        .iter()
        .find(|r| r.name == "read_limits")
        .expect("reader row");
    assert_eq!(
        reader_row.fired, "structural",
        "a configuration read is not state"
    );
    assert!(rep.to_xml_string().contains("state_lens=\"mutable\""));
}

#[test]
fn test_constant_the_corpus_mutates_is_state() {
    let interner = Interner::new();
    let writer = function(&interner, 0, "reset_limits");
    let reader = function(&interner, 1, "read_limits");
    let mut limits = Symbol::new(
        2,
        interner.intern("LIMITS"),
        interner.intern("src/lib.rs::LIMITS"),
        SymKind::Var,
        Lang::Rust,
        0,
        120,
    );
    limits.binding = Binding::Const;
    let mut ingest = ingest_for(&interner, vec![writer, reader, limits]);
    ingest.references.push(Reference::new(
        0,
        interner.intern("LIMITS"),
        0,
        10,
        Lang::Rust,
        RefRole::Write,
    ));
    ingest.references.push(Reference::new(
        1,
        interner.intern("LIMITS"),
        0,
        20,
        Lang::Rust,
        RefRole::Read,
    ));
    let graph = Graph::build(&ingest, &interner);
    let rep = panel(
        &ingest,
        &graph,
        &interner,
        Some("lenient"),
        Err(NO_HISTORY.to_string()),
    );
    let reader_row = rep
        .rows
        .iter()
        .find(|r| r.name == "read_limits")
        .expect("reader row");
    assert!(reader_row.fired.contains("state"));
}
