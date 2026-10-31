use ripwire_rs::query::navigation::{NavContext, NavMode, execute_nav_symbol_query_in_root};
use ripwire_rs::serialize::PageRequest;
use ripwire_rs::{Graph, Interner, Lang, LanguageRegistry, PageRankConfig, ingest_in_memory_files};

const PROCESS_A: &[u8] =
    b"def process(x):\n    return x\n\ndef helper_a():\n    return process(1)\n";
const PROCESS_B: &[u8] = b"def process(y):\n    return y\n";
const SPREAD: &[u8] =
    b"def run_one():\n    return process(3)\n\ndef run_two():\n    return process(4)\n\nprocess(5)\n";
const SINGLE: &[u8] = b"def run_three():\n    return process(6)\n";

const DC_ROW: &str = "<dc in=\"";

fn render(files: &[(&str, Lang, &[u8])], mode: NavMode, sym: &str, page: PageRequest) -> String {
    let reg = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(files, &reg, &interner);
    let graph = Graph::build(&ingest, &interner);
    let pr = graph.page_rank(&PageRankConfig::default());
    let ctx = NavContext {
        ingest: &ingest,
        graph: &graph,
        ranks: &pr.ranks,
        interner: &interner,
        page,
        max_depth: None,
        pr_iters: Some(pr.iterations),
    };
    let mut out = Vec::new();
    execute_nav_symbol_query_in_root(".", sym, mode, &ctx, &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

fn two_defs_fixture() -> Vec<(&'static str, Lang, &'static [u8])> {
    vec![
        ("alpha/a.py", Lang::Python, PROCESS_A),
        ("beta/b.py", Lang::Python, PROCESS_B),
        ("gamma/c.py", Lang::Python, SPREAD),
        ("delta/d.py", Lang::Python, SINGLE),
    ]
}

fn page(limit: Option<usize>, offset: Option<usize>) -> PageRequest {
    PageRequest {
        limit,
        offset,
        budget: None,
    }
}

#[test]
fn impact_lists_declined_call_sites_file_first() {
    let xml = render(
        &two_defs_fixture(),
        NavMode::Impact,
        "process",
        PageRequest::default(),
    );
    assert!(xml.contains(" defs=\"2\""), "{xml}");
    assert!(xml.contains(" declined_calls=\"4\" root="), "{xml}");
    assert!(!xml.contains(" dc_shown=\""), "{xml}");
    let rows: Vec<&str> = xml.match_indices(DC_ROW).map(|(i, _)| &xml[i..]).collect();
    let expected = [
        "<dc in=\"run_three\" p=\"delta/d.py:2\" cands=\"2\" via=\"declined\"/>",
        "<dc in=\"run_one\" p=\"gamma/c.py:2\" cands=\"2\" via=\"declined\"/>",
        "<dc in=\"run_two\" p=\"gamma/c.py:5\" cands=\"2\" via=\"declined\"/>",
        "<dc in=\"&lt;file-scope&gt;\" p=\"gamma/c.py:7\" cands=\"2\" via=\"declined\"/>",
    ];
    assert_eq!(rows.len(), expected.len(), "{xml}");
    for (row, want) in rows.iter().zip(expected) {
        assert!(row.starts_with(want), "{row} != {want}");
    }
}

#[test]
fn impact_declined_rows_follow_the_budget_and_point_at_every_site() {
    let tiny = PageRequest {
        budget: Some(1),
        ..page(Some(1), Some(1))
    };
    let xml = render(&two_defs_fixture(), NavMode::Impact, "process", tiny);
    assert!(
        xml.contains(" declined_calls=\"4\" dc_shown=\"1\" root="),
        "{xml}"
    );
    assert_eq!(xml.matches(DC_ROW).count(), 1, "{xml}");
}

#[test]
fn impact_declined_rows_fit_the_default_byte_budget() {
    let many: String = (0..20).fold(String::from("def many():\n"), |acc, i| {
        acc + &format!("    process({i})\n")
    });
    let mut files = two_defs_fixture();
    files.push(("omega/m.py", Lang::Python, many.as_bytes()));
    let xml = render(&files, NavMode::Impact, "process", PageRequest::default());
    assert!(xml.contains(" declined_calls=\"24\" root="), "{xml}");
    assert!(!xml.contains(" dc_shown=\""), "{xml}");
    assert_eq!(xml.matches(DC_ROW).count(), 24, "{xml}");
}

const CHAIN: &[u8] = b"def base():\n    return 1\n\ndef mid_a():\n    return base()\n\ndef mid_b():\n    return base()\n\ndef top():\n    return mid_a()\n";

#[test]
fn impact_rows_carry_depth_only_where_it_changes() {
    let files = [("pkg/core.py", Lang::Python, CHAIN)];
    let xml = render(&files, NavMode::Impact, "base", PageRequest::default());
    assert!(xml.contains(" defs=\"1\" reaches=\"3\""), "{xml}");
    assert!(!xml.contains("<def "), "{xml}");
    assert!(!xml.contains(" dist="), "{xml}");
    assert!(!xml.contains("<files "), "{xml}");
    assert_eq!(xml.matches(" d=\"1\"").count(), 1, "{xml}");
    assert_eq!(xml.matches(" d=\"2\"").count(), 1, "{xml}");
}

#[test]
fn impact_lists_matched_defs_and_summarises_files_when_capped() {
    let xml = render(
        &two_defs_fixture(),
        NavMode::Impact,
        "process",
        PageRequest::default(),
    );
    assert!(
        xml.contains("<def ix=\"1\" t=\"fn\" p=\"alpha/a.py:1\"/>"),
        "{xml}"
    );
    assert!(
        xml.contains("<def ix=\"2\" t=\"fn\" p=\"beta/b.py:1\"/>"),
        "{xml}"
    );

    let files = [("pkg/core.py", Lang::Python, CHAIN)];
    let capped = render(&files, NavMode::Impact, "base", page(Some(1), Some(2)));
    assert!(
        capped.contains(" shown=\"1\" capped=\"1\" total=\"3\" has_more=\"0\" next_offset=\"3\" offset=\"2\" limit=\"1\""),
        "{capped}"
    );
    assert!(
        capped.contains("<files files=\"1\" shown_files=\"1\" files_capped=\"0\"><rf p=\"pkg/core.py\" syms=\"3\" d=\"1\"/></files>"),
        "{capped}"
    );
    assert!(
        capped.contains(" n=\"top\" p=\"pkg/core.py:10\" d=\"2\""),
        "{capped}"
    );
}

#[test]
fn uses_counts_matched_defs_and_pages_rows() {
    let all = render(
        &two_defs_fixture(),
        NavMode::Uses,
        "process",
        PageRequest::default(),
    );
    assert!(
        all.contains(" defs=\"2\" external=\"0\" count=\"5\""),
        "{all}"
    );
    assert!(!all.contains(" shown="), "{all}");
    assert_eq!(all.matches("<u ").count(), 5, "{all}");

    let window = render(
        &two_defs_fixture(),
        NavMode::Uses,
        "process",
        page(Some(2), Some(1)),
    );
    assert!(
        window.contains(
            " shown=\"2\" capped=\"1\" total=\"5\" has_more=\"1\" next_offset=\"3\" offset=\"1\" limit=\"2\""
        ),
        "{window}"
    );
    let rows: Vec<&str> = window
        .lines()
        .filter(|l| l.trim_start().starts_with("<u "))
        .collect();
    assert_eq!(
        rows,
        [
            "  <u role=\"call\" p=\"delta/d.py:2\" in_id=\"run_three\"/>",
            "  <u role=\"call\" p=\"gamma/c.py:2\" in_id=\"run_one\"/>",
        ],
        "{window}"
    );
}

#[test]
fn impact_declined_calls_exclude_ambiguous_table_reads() {
    let files = [
        (
            "alpha/schema.sql",
            Lang::Sql,
            b"CREATE TABLE users (id INT);" as &[u8],
        ),
        (
            "beta/schema.sql",
            Lang::Sql,
            b"CREATE TABLE users (id INT);",
        ),
        (
            "gamma/client.py",
            Lang::Python,
            b"def fetch():\n    return \"SELECT id FROM users\"\n",
        ),
    ];
    let xml = render(&files, NavMode::Impact, "users", PageRequest::default());
    assert!(!xml.contains(DC_ROW), "{xml}");
    assert!(!xml.contains(" declined_calls=\""), "{xml}");
}

#[test]
fn impact_importers_are_listed_whatever_the_reach_page() {
    let files = [
        (
            "core.py",
            Lang::Python,
            b"def process():\n    return 1\n" as &[u8],
        ),
        ("a.py", Lang::Python, b"from core import process\n"),
        ("b.py", Lang::Python, b"from core import process\n"),
        ("c.py", Lang::Python, b"from core import process\n"),
    ];
    let paged = render(&files, NavMode::Impact, "process", page(Some(1), Some(1)));
    assert!(
        paged.contains(" importers=\"3\" shown_importers=\"3\""),
        "{paged}"
    );
    for importer in ["a.py", "b.py", "c.py"] {
        assert!(
            paged.contains(&format!("<f via=\"import\" p=\"{importer}\"")),
            "{paged}"
        );
    }
}

#[test]
fn shared_impact_depth_changes_the_reachable_set() {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let files = [("pkg/core.py", Lang::Python, CHAIN)];
    let ingest = ingest_in_memory_files(&files, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    let ranks = graph.page_rank(&PageRankConfig::default()).ranks;
    let mut ctx = NavContext {
        ingest: &ingest,
        graph: &graph,
        ranks: &ranks,
        interner: &interner,
        page: PageRequest::default(),
        max_depth: Some(1),
        pr_iters: None,
    };
    let mut shallow = Vec::new();
    execute_nav_symbol_query_in_root(".", "base", NavMode::Impact, &ctx, &mut shallow).unwrap();
    let shallow = String::from_utf8(shallow).unwrap();
    assert!(shallow.contains("reaches=\"2\""), "{shallow}");
    assert!(shallow.contains(" n=\"mid_a\""), "{shallow}");
    assert!(shallow.contains(" n=\"mid_b\""), "{shallow}");
    assert!(!shallow.contains(" n=\"top\""), "{shallow}");
    assert!(!shallow.contains(" pr_iters="), "{shallow}");

    ctx.max_depth = Some(2);
    ctx.pr_iters = Some(7);
    let mut deeper = Vec::new();
    execute_nav_symbol_query_in_root(".", "base", NavMode::Impact, &ctx, &mut deeper).unwrap();
    let deeper = String::from_utf8(deeper).unwrap();
    assert!(deeper.contains("reaches=\"3\""), "{deeper}");
    assert!(deeper.contains(" n=\"top\""), "{deeper}");
    assert!(deeper.contains(" pr_iters=\"7\""), "{deeper}");
}

fn impact_row<'a>(xml: &'a str, name: &str) -> &'a str {
    let marker = format!("<s t=\"fn\" n=\"{name}\"");
    let start = xml
        .find(&marker)
        .unwrap_or_else(|| panic!("no row for {name}: {xml}"));
    let rest = &xml[start..];
    &rest[..rest.find("/>").unwrap() + 2]
}

#[test]
fn a_caller_bound_by_name_alone_is_marked_via_name() {
    let files = [
        (
            "lib/target.py",
            Lang::Python,
            b"def target():\n    return 1\n\ndef sibling():\n    return target()\n".as_slice(),
        ),
        (
            "app/other.py",
            Lang::Python,
            b"def stranger():\n    return target()\n".as_slice(),
        ),
    ];
    let xml = render(&files, NavMode::Impact, "target", PageRequest::default());
    assert!(
        impact_row(&xml, "stranger").contains("via=\"name\""),
        "{xml}"
    );
    assert!(xml.contains(" reaches=\"1\" hedged=\"1\""), "{xml}");
    assert!(!impact_row(&xml, "sibling").contains("via="), "{xml}");
}
