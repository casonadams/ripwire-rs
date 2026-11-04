//! Paging honesty: a command that declares a windowed shared option must show one row of several at `limit=1`.
//! Each command runs in memory through its MCP tool (`dispatch`) or its public render; commands that cannot run in
//! memory are allowlisted with a reason, and stale or missing entries fail. `McpPaged` commands are windowed on the
//! MCP tool only (the CLI report does not window), so they are asserted through their tool.
use ripwire_rs::commands::context::{ExecutionContext, OutputDialect};
use ripwire_rs::commands::{
    self, Command, SharedOption, ast_match, biggest_first, communities, context_ratio, graph_query,
    tree,
};
use ripwire_rs::metrics::lint::{LintOptions, run_lint_scan, serialize_lint_xml};
use ripwire_rs::query::commands::{QueryContext, dispatch};
use ripwire_rs::serialize::{PageRequest, XmlWriter};
use ripwire_rs::{
    CommitInfo, Graph, IngestResult, Interner, Lang, LanguageRegistry, PageRankConfig,
    PageRankResult, ingest_in_memory_files, parse_from,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::io;

const FLOW: &str = "pub fn target() {}\nfn a() { target(); }\nfn b() { target(); }\nfn c() { target(); }\nfn hub() { target(); a(); b(); }\ntrait Shape { fn area(&self) -> u32; }\nstruct Sq;\nstruct Ci;\nimpl Shape for Sq { fn area(&self) -> u32 { 1 } }\nimpl Shape for Ci { fn area(&self) -> u32 { 2 } }\n";
const OTHER: &str = "pub fn other() {}\nfn user() { other(); }\n";

/// Commands that cannot be windowed in memory, each with the concrete reason.
const ALLOWLIST: &[(&str, &str)] = &[
    (
        "clones",
        "CLI path takes the repo root and reads sources from disk; no in-memory input traced",
    ),
    (
        "comment-coherence",
        "render calls compute_comment_coherence_in_root(root), which reads comments from disk",
    ),
    (
        "cochange",
        "CLI path mines co-change pairs from git history under root; no in-memory commit fixture for pairs",
    ),
    (
        "dead-code",
        "execute_dead_code_query_paged builds FileSources from root (disk reads)",
    ),
    (
        "for",
        "CliPaged: its MCP tool exposes no limit; paging is covered in tests/cli/for_window.rs",
    ),
    (
        "report",
        "CliPaged markdown report: the XML row checker does not apply; windowing is asserted in tests/metrics/report_paging.rs",
    ),
    (
        "doc-drift",
        "CliPaged: its MCP tool exposes no limit; the CLI report reads markdown and history from disk",
    ),
    (
        "exemplar",
        "McpPaged: returns one exemplar per query, so there is no second row to window",
    ),
    (
        "external-surface",
        "fixture has no unresolved external references, so no row to window (not traced further)",
    ),
    (
        "flags",
        "MCP tool harvests header gates and env reads; fixture contains none (not traced further)",
    ),
    (
        "gate",
        "subcommands (clones, quality) load the repo from root through disk-based helpers",
    ),
    (
        "hotspots",
        "walks git history under root (resolve_cutoff_timestamp, execute_with_git_state_fmt)",
    ),
    (
        "navigate",
        "CLI navigate runs through run_nav and loads the repo from disk; no MCP tool of its own",
    ),
    (
        "nonlocal-state",
        "fixture has no non-local mutable state, so there is no second row to window",
    ),
    (
        "seams",
        "render is private (fn render in seams.rs); no public entry point",
    ),
    (
        "stray-content",
        "MCP tool reads divergent-branch lines from git through call_stray_content(root)",
    ),
    (
        "test-gate",
        "execute_test_gate_query_paged resolves changed files from root (git) before windowing",
    ),
    (
        "whereis",
        "CliPaged: its MCP tool exposes no limit; the CLI report loads the repo from disk",
    ),
    (
        "naming-consistency",
        "in-memory fixture yields a single naming kind (one vote row); a window needs a second kind",
    ),
    (
        "deps",
        "in-memory fixture has no file-to-file imports, so the ranked file list has no second row to window",
    ),
    (
        "quality-panel",
        "in-memory fixture fires no quality family row (total=0); needs a richer fixture",
    ),
];

type Render = fn(&World, &str) -> String;

struct Case {
    command: &'static str,
    tool: Option<&'static str>,
    render: Render,
}

const CASES: &[Case] = &[
    Case {
        command: "callers",
        tool: Some("callers"),
        render: |w, c| mcp(w, c, "callers", &json!({"symbol": "target", "limit": 1})),
    },
    Case {
        command: "callers",
        tool: Some("find_referencing_symbols"),
        render: |w, c| {
            mcp(
                w,
                c,
                "find_referencing_symbols",
                &json!({"symbol": "target", "limit": 1}),
            )
        },
    },
    Case {
        command: "callees",
        tool: Some("callees"),
        render: |w, c| mcp(w, c, "callees", &json!({"symbol": "hub", "limit": 1})),
    },
    Case {
        command: "find-symbol",
        tool: Some("find_symbol"),
        render: |w, c| {
            mcp(
                w,
                c,
                "find_symbol",
                &json!({"symbol": "target", "limit": 1}),
            )
        },
    },
    Case {
        command: "uses",
        tool: Some("uses"),
        render: |w, c| mcp(w, c, "uses", &json!({"symbol": "target", "limit": 1})),
    },
    Case {
        command: "impact",
        tool: Some("impact"),
        render: |w, c| mcp(w, c, "impact", &json!({"symbol": "target", "limit": 1})),
    },
    Case {
        command: "grep",
        tool: Some("grep"),
        render: |w, c| mcp(w, c, "grep", &json!({"pattern": "target", "limit": 1})),
    },
    Case {
        command: "lego",
        tool: Some("lego"),
        render: |w, c| mcp(w, c, "lego", &json!({"interface": "Shape", "limit": 1})),
    },
    Case {
        command: "owners",
        tool: Some("owners"),
        render: |w, c| mcp(w, c, "owners", &json!({"detail": 1, "limit": 1})),
    },
    Case {
        command: "edit_check",
        tool: Some("edit_check"),
        render: |w, c| mcp(w, c, "edit_check", &json!({"symbol": "target", "limit": 1})),
    },
    Case {
        command: "lint",
        tool: None,
        render: |_, _| lint(),
    },
    Case {
        command: "match",
        tool: None,
        render: |w, _| ast_match_xml(w),
    },
    Case {
        command: "graph-query",
        tool: None,
        render: |w, _| graph_query_xml(w),
    },
    Case {
        command: "tree",
        tool: None,
        render: |w, _| cli(w, |ctx, out| tree::render(ctx, &Default::default(), out)),
    },
    Case {
        command: "context-ratio",
        tool: None,
        render: |w, _| {
            cli(w, |ctx, out| {
                context_ratio::render(ctx, &Default::default(), out)
            })
        },
    },
    Case {
        command: "communities",
        tool: None,
        render: |w, _| {
            cli(w, |ctx, out| {
                communities::render(ctx, &Default::default(), out)
            })
        },
    },
    Case {
        command: "biggest-first",
        tool: None,
        render: |w, _| {
            cli(w, |ctx, out| {
                biggest_first::render(ctx, &Default::default(), out)
            })
        },
    },
];

struct World {
    interner: Interner,
    ingest: IngestResult,
    graph: Graph,
    pr: PageRankResult,
    sources: [(&'static str, &'static [u8]); 2],
    commits: Vec<CommitInfo>,
}

impl World {
    fn new() -> Self {
        let registry = LanguageRegistry::new().unwrap();
        let interner = Interner::new();
        let files = [
            ("src/flow.rs", Lang::Rust, FLOW.as_bytes()),
            ("src/other.rs", Lang::Rust, OTHER.as_bytes()),
        ];
        let ingest = ingest_in_memory_files(&files, &registry, &interner);
        let graph = Graph::build(&ingest, &interner);
        let pr = graph.page_rank(&PageRankConfig::default());
        Self {
            interner,
            ingest,
            graph,
            pr,
            sources: [
                ("src/flow.rs", FLOW.as_bytes()),
                ("src/other.rs", OTHER.as_bytes()),
            ],
            commits: vec![CommitInfo {
                id: "rev".into(),
                timestamp: 1_700_000_000,
                author: "Ada".into(),
                changed_files: vec!["src/flow.rs".into(), "src/other.rs".into()],
            }],
        }
    }

    fn query(&self) -> QueryContext<'_> {
        QueryContext {
            root: ":memory:",
            ingest: &self.ingest,
            graph: &self.graph,
            pr: &self.pr,
            interner: &self.interner,
            commits: Some(&self.commits[..]),
            sources: &self.sources,
            at: None,
        }
    }
}

fn one() -> PageRequest {
    PageRequest {
        limit: Some(1),
        offset: None,
        budget: None,
    }
}

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap()
}

fn mcp(world: &World, command: &str, tool: &str, args: &Value) -> String {
    let query = world.query();
    let value = dispatch(&query, commands::tools(), tool, args)
        .unwrap_or_else(|error| panic!("--{command}: tool {tool} rejected {args}: {error}"));
    value
        .as_str()
        .unwrap_or_else(|| panic!("--{command}: tool {tool} returned no XML: {value}"))
        .to_owned()
}

fn cli(
    world: &World,
    render: impl FnOnce(&ExecutionContext<'_>, &mut XmlWriter<'_, Vec<u8>>) -> io::Result<()>,
) -> String {
    let cli = parse_from(["rw", "--limit=1"]).unwrap();
    let ctx = ExecutionContext {
        cli: &cli,
        root: ":memory:",
        ingest: &world.ingest,
        graph: &world.graph,
        pr: &world.pr,
        interner: &world.interner,
    };
    let mut bytes = Vec::new();
    let mut out = cli.wrap_xml_writer(&mut bytes, false);
    render(&ctx, &mut out).unwrap();
    out.finish().unwrap();
    drop(out);
    text(bytes)
}

fn lint() -> String {
    const SOURCE: &str = "void track() { strcpy(a, b); strcpy(c, d); }";
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[("pkg/track.c", Lang::C, SOURCE.as_bytes())],
        &registry,
        &interner,
    );
    let scan = run_lint_scan(
        &ingest,
        &interner,
        &|path| (path == "pkg/track.c").then(|| SOURCE.to_owned()),
        LintOptions::default(),
    )
    .unwrap();
    let mut bytes = Vec::new();
    serialize_lint_xml(".", &scan, one(), &mut bytes).unwrap();
    text(bytes)
}

fn ast_match_xml(world: &World) -> String {
    let query = world.query();
    let mut out = Vec::new();
    ast_match::execute(
        &query,
        "(function_item name: (identifier) @name)",
        one(),
        &mut out,
    )
    .unwrap();
    text(out)
}

fn graph_query_xml(world: &World) -> String {
    let query = world.query();
    let mut out = Vec::new();
    graph_query::execute(&query, "not(name(\"zzz\"))", one(), &mut out).unwrap();
    text(out)
}

/// Every `name="value"` pair in `xml`, in document order.
fn attributes(xml: &str) -> Vec<(&str, &str)> {
    xml.match_indices("=\"")
        .map(|(eq, _)| {
            let start = xml[..eq]
                .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .map_or(0, |i| i + 1);
            let value_start = eq + 2;
            let value_end = xml[value_start..]
                .find('"')
                .map_or(xml.len(), |i| value_start + i);
            (&xml[start..eq], &xml[value_start..value_end])
        })
        .collect()
}

/// A window is disclosed when some `shown*` attribute reads 1 beside a `total*` count above 1. Commands
/// spell the pair differently (`shown`/`total`, `shown_syms`/`total_syms`, `shown_unflagged`/`total`).
fn one_of_many(command: &str, xml: &str) -> Result<(), String> {
    let attrs = attributes(xml);
    let shown = attrs
        .iter()
        .any(|(name, value)| name.starts_with("shown") && *value == "1");
    let total = attrs.iter().any(|(name, value)| {
        (*name == "total" || name.starts_with("total_") || name.ends_with("_total"))
            && value.parse::<usize>().is_ok_and(|total| total > 1)
    });
    if shown && total {
        return Ok(());
    }
    let head: String = xml.chars().take(300).collect();
    Err(format!(
        "--{command}: limit=1 must show one of several rows (no shown*=1 with total*>1); output: {head}"
    ))
}

fn windowed(command: &dyn Command) -> bool {
    command.options().iter().any(|option| {
        matches!(
            option,
            SharedOption::Paged
                | SharedOption::PagedMatches
                | SharedOption::CliPaged
                | SharedOption::McpPaged
        )
    })
}

#[test]
fn every_paged_command_is_exercised_or_allowlisted() {
    let exercised: BTreeSet<&str> = CASES.iter().map(|case| case.command).collect();
    let allowed: BTreeSet<&str> = ALLOWLIST.iter().map(|&(name, _)| name).collect();
    let mut paged = BTreeSet::new();
    for command in commands::commands().iter().copied() {
        let name = command.name();
        if name.is_empty() || !windowed(command) {
            continue;
        }
        paged.insert(name);
        if exercised.contains(name) {
            for tool in command.tools() {
                assert!(
                    CASES
                        .iter()
                        .any(|case| case.command == name && case.tool == Some(tool.name)),
                    "--{name}: MCP tool {} has no in-memory case",
                    tool.name
                );
            }
        } else {
            assert!(
                allowed.contains(name),
                "--{name} is paged but neither exercised in memory nor allowlisted"
            );
        }
    }
    for case in CASES {
        assert!(
            paged.contains(case.command),
            "--{} has an in-memory case but is not a paged command",
            case.command
        );
        assert!(
            !allowed.contains(case.command),
            "--{} is both exercised and allowlisted",
            case.command
        );
    }
    for &(name, reason) in ALLOWLIST {
        assert!(
            paged.contains(name),
            "allowlist entry --{name} is not a paged command"
        );
        assert!(
            !exercised.contains(name),
            "allowlist entry --{name} is exercised in memory; remove it"
        );
        assert!(
            !reason.trim().is_empty(),
            "allowlist entry --{name} has no reason"
        );
    }
}

#[test]
fn paged_commands_show_one_row_of_many_under_limit_one() {
    let world = World::new();
    let failures: Vec<String> = CASES
        .iter()
        .filter_map(|case| one_of_many(case.command, &(case.render)(&world, case.command)).err())
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
