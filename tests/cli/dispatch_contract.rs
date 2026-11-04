use ripwire_rs::commands::context::ExecutionContext;
use ripwire_rs::commands::{
    around, biggest_first, communities, context_ratio, external_surface, naming_consistency,
    quality_panel, report, symbol_metrics, tree,
};
use ripwire_rs::commands::{
    around::AroundArgs, biggest_first::BiggestFirstArgs, communities::CommunitiesArgs,
    context_ratio::ContextRatioArgs, external_surface::ExternalSurfaceArgs,
    naming_consistency::NamingConsistencyArgs, quality_panel::QualityPanelArgs,
    symbol_metrics::SymbolMetricsArgs,
};
use ripwire_rs::serialize::XmlWriter;
use ripwire_rs::{Graph, Interner, Lang, LanguageRegistry, PageRankConfig, ingest_in_memory_files};
use std::collections::BTreeSet;

fn render(
    args: &[&str],
    body: impl FnOnce(&ExecutionContext<'_>, &mut XmlWriter<'_, Vec<u8>>) -> std::io::Result<()>,
) -> std::io::Result<String> {
    let cli = ripwire_rs::parse_from(std::iter::once("rw").chain(args.iter().copied())).unwrap();
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[(
            "src/lib.rs",
            Lang::Rust,
            b"fn entry() { helper(); }\nfn helper() {}\nfn isolated() {}\n".as_slice(),
        )],
        &registry,
        &interner,
    );
    let graph = Graph::build(&ingest, &interner);
    let pr = graph.page_rank(&PageRankConfig::default());
    let ctx = ExecutionContext {
        cli: &cli,
        root: ":memory:",
        ingest: &ingest,
        graph: &graph,
        pr: &pr,
        interner: &interner,
    };
    let mut output = Vec::new();
    {
        let mut writer = XmlWriter::new(&mut output, true);
        body(&ctx, &mut writer)?;
        writer.finish()?;
    }
    Ok(String::from_utf8(output).unwrap())
}

fn names(xml: &str) -> BTreeSet<String> {
    roxmltree::Document::parse(xml)
        .unwrap()
        .descendants()
        .filter_map(|node| node.attribute("n"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn file_tree_retains_definitions_without_inventing_call_edges() {
    let output = render(&["--tree"], |ctx, out| {
        tree::render(ctx, &tree::TreeArgs::default(), out)
    })
    .unwrap();
    assert_eq!(
        names(&output),
        ["entry", "helper", "isolated"].map(str::to_owned).into()
    );
    let document = roxmltree::Document::parse(&output).unwrap();
    assert!(
        document
            .descendants()
            .any(|node| node.attribute("p") == Some("src/lib.rs"))
    );
}

#[test]
fn readability_inventory_does_not_lose_unreachable_functions() {
    let output = render(&["--biggest-first"], |ctx, out| {
        biggest_first::render(
            ctx,
            &ctx.cli.plugins.decode::<BiggestFirstArgs>().unwrap(),
            out,
        )
    })
    .unwrap();
    assert_eq!(
        names(&output),
        ["entry", "helper", "isolated"].map(str::to_owned).into()
    );
}

#[test]
fn invalid_slice_requests_fail_before_any_repository_acquisition() {
    for (args, message) in [
        (
            vec!["rw", "--slice", "entry:value", "--slice-flow", "sideways"],
            "invalid slice flow",
        ),
        (vec!["rw", "slice"], "slice target is required"),
    ] {
        let cli = ripwire_rs::parse_from(args).unwrap();
        let error = ripwire_rs::run_cli_args(cli).unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn community_dispatch_separates_an_isolated_definition_from_a_connected_pair() {
    let output = render(&["--communities"], |ctx, out| {
        communities::render(
            ctx,
            &ctx.cli.plugins.decode::<CommunitiesArgs>().unwrap(),
            out,
        )
    })
    .unwrap();
    let document = roxmltree::Document::parse(&output).unwrap();
    let summary = document
        .descendants()
        .find(|node| node.has_tag_name("communities"))
        .unwrap();
    assert_eq!(summary.attribute("isolated_source"), Some("1"));
    let reported = names(&output);
    assert!(reported.contains("entry"));
    assert!(reported.contains("helper"));
}

#[test]
fn ego_graph_dispatch_excludes_disconnected_functions() {
    let output = render(&["--around", "entry"], |ctx, out| {
        around::render(ctx, &ctx.cli.plugins.decode::<AroundArgs>().unwrap(), out)
    })
    .unwrap();
    assert_eq!(
        names(&output),
        ["entry", "helper"].map(str::to_owned).into()
    );
}

#[test]
fn external_surface_does_not_reclassify_a_resolved_internal_call() {
    let output = render(&["--external-surface"], |ctx, out| {
        external_surface::render(
            ctx,
            &ctx.cli.plugins.decode::<ExternalSurfaceArgs>().unwrap(),
            out,
        )
    })
    .unwrap();
    let document = roxmltree::Document::parse(&output).unwrap();
    assert!(!document.descendants().any(|node| node.has_tag_name("x")));
}

#[test]
fn naming_votes_count_only_the_selected_language_definitions() {
    let output = render(&["--naming-consistency"], |ctx, out| {
        naming_consistency::render(
            ctx,
            &ctx.cli.plugins.decode::<NamingConsistencyArgs>().unwrap(),
            out,
        )
    })
    .unwrap();
    let document = roxmltree::Document::parse(&output).unwrap();
    let votes: Vec<_> = document
        .descendants()
        .filter(|node| node.has_tag_name("vote"))
        .map(|node| {
            (
                node.attribute("kind"),
                node.attribute("style"),
                node.attribute("count"),
            )
        })
        .collect();
    assert_eq!(votes, [(Some("fn"), Some("lower"), Some("3"))]);
}

#[test]
fn executive_report_ranks_the_reached_helper_ahead_of_uncalled_entries() {
    let output = render(&["--report"], report::render).unwrap();
    let rows: Vec<_> = output
        .lines()
        .filter(|line| line.starts_with("| `"))
        .map(|line| line.split('|').nth(1).unwrap().trim().trim_matches('`'))
        .collect();
    assert_eq!(rows, ["helper", "entry", "isolated"]);
}

#[test]
fn context_ratio_counts_internal_entities_without_external_context() {
    let output = render(&["--context-ratio"], |ctx, out| {
        context_ratio::render(
            ctx,
            &ctx.cli.plugins.decode::<ContextRatioArgs>().unwrap(),
            out,
        )
    })
    .unwrap();
    let document = roxmltree::Document::parse(&output).unwrap();
    assert_eq!(document.root_element().attribute("units"), Some("3"));
    let row = document
        .descendants()
        .find(|node| node.has_tag_name("s"))
        .unwrap();
    assert_eq!(row.attribute("n"), Some("entry"));
    assert_eq!(row.attribute("ents"), Some("1"));
    assert_eq!(row.attribute("ents_out"), Some("0"));
}

#[test]
fn quality_panel_does_not_rank_a_small_healthy_call_graph_as_a_problem() {
    let output = render(&["--quality-panel"], |ctx, out| {
        quality_panel::render(
            ctx,
            &ctx.cli.plugins.decode::<QualityPanelArgs>().unwrap(),
            out,
        )
    })
    .unwrap();
    let document = roxmltree::Document::parse(&output).unwrap();
    let report = document.root_element();
    assert_eq!(report.attribute("eligible"), Some("3"));
    assert_eq!(report.attribute("ranked"), Some("0"));
}

#[test]
fn metrics_reports_reference_direction_not_just_symbol_presence() {
    let output = render(&["--metrics"], |ctx, out| {
        symbol_metrics::render(
            ctx,
            &ctx.cli.plugins.decode::<SymbolMetricsArgs>().unwrap(),
            out,
        )
    })
    .unwrap();
    let document = roxmltree::Document::parse(&output).unwrap();
    let helper = document
        .descendants()
        .find(|node| node.attribute("n") == Some("helper"))
        .unwrap();
    let entry = document
        .descendants()
        .find(|node| node.attribute("n") == Some("entry"))
        .unwrap();
    assert_eq!(helper.attribute("in"), Some("1"));
    assert_eq!(helper.attribute("out"), Some("0"));
    assert_eq!(entry.attribute("in"), Some("0"));
    assert_eq!(entry.attribute("out"), Some("1"));
}

fn selected<A: clap::FromArgMatches + 'static>(
    cli: &ripwire_rs::Cli,
    plugin: &ripwire_rs::commands::Plugin<A>,
) -> bool {
    cli.plugins
        .decode::<A>()
        .is_some_and(|args| (plugin.selected)(&args))
}

fn assert_group(group: &dyn Fn(&ripwire_rs::Cli) -> bool, on: &[&str], off: &[&str]) {
    let active = |flag: &str| group(&ripwire_rs::parse_from(["rw", flag]).unwrap());
    for flag in on {
        assert!(active(flag), "{flag}");
    }
    for flag in off {
        assert!(!active(flag), "{flag}");
    }
}

/// A selector that is true when the command line selects any of the listed plugins.
macro_rules! selects {
    ($($plugin:path),+ $(,)?) => {
        |cli: &ripwire_rs::Cli| [$(selected(cli, &$plugin)),+].into_iter().any(|on| on)
    };
}

#[test]
fn primary_flags_select_only_their_group() {
    use ripwire_rs::commands as c;
    use ripwire_rs::commands::{
        arch, around, biggest_first, comment_coherence, communities, context_ratio, dead_code,
        deps, expand, external_surface, field_affinity, legend_dict, lint, lint_catalog, mermaid,
        naming_consistency, nonlocal_state, quality_panel, report, symbol_metrics, tree,
    };
    let detail = selects!(
        expand::EXPAND,
        tree::TREE,
        biggest_first::BIGGEST_FIRST,
        nonlocal_state::NONLOCAL_STATE,
        lint::LINT,
        lint_catalog::LINT_CATALOG,
        naming_consistency::NAMING_CONSISTENCY,
        report::REPORT,
        legend_dict::LEGEND_DICT,
    );
    assert_group(
        &detail,
        &[
            "--expand=a",
            "--tree",
            "--biggest-first",
            "--nonlocal-state",
            "--lint",
            "--lint-catalog",
            "--naming-consistency",
            "--report",
            "--legend-dict",
        ],
        &[
            "--lint-select=a",
            "--lint-ignore=a",
            "--lint-max-per-rule=1",
        ],
    );
    let workspace = selects!(
        c::owners::OWNERS,
        c::stray_content::STRAY_CONTENT,
        c::replace_symbol_body::REPLACE_SYMBOL_BODY,
        c::insert_before_symbol::INSERT_BEFORE_SYMBOL,
        c::insert_after_symbol::INSERT_AFTER_SYMBOL,
        c::whereis::WHEREIS,
        c::doc_drift::DOC_DRIFT,
        c::flags_report::FLAGS,
        c::mentions::MENTIONS,
        c::doctor::DOCTOR,
        c::handoff::HANDOFF,
        c::notes::NOTES,
        c::note_add::NOTE_ADD,
        c::flip::FLIP,
    );
    assert_group(
        &workspace,
        &[
            "--whereis=a",
            "--mentions=a",
            "--note-add=a",
            "--flip=a",
            "--owners",
            "--doc-drift",
            "--flags",
            "--stray-content",
            "--doctor",
            "--handoff",
            "--notes",
            "--replace-symbol-body=a:b",
            "--insert-before-symbol=a:b",
            "--insert-after-symbol=a:b",
        ],
        &["--whereis-full-text", "--whereis-listing=defs"],
    );
    let safety = selects!(
        c::test_gate::TEST_GATE,
        c::situ::SITU,
        c::safe_delete::SAFE_DELETE,
        c::exercises::EXERCISES,
        c::pr_context::PR_CONTEXT,
        c::merge_scout::MERGE_SCOUT,
        c::dmm::DMM,
        c::plan_lanes::PLAN_LANES,
        c::scan_skills::SCAN_SKILLS,
        c::edit_payload::EDIT_PAYLOAD,
        c::partition::PARTITION,
        c::seams::SEAMS,
        c::baseline::BASELINE,
    );
    assert_group(
        &safety,
        &[
            "--test-gate=a",
            "--situ=a",
            "--safe-delete=a",
            "--exercises=a",
            "--pr-context=a",
            "--merge-scout=a",
            "--dmm=a",
            "--plan-lanes",
            "--scan-skills=a",
            "--edit-payload=a",
            "--partition",
            "--seams",
            "--baseline",
            "--baseline-update",
        ],
        &["--owners", "--doc-drift", "--flags", "--lint-select=a"],
    );
    let architecture = selects!(
        dead_code::DEAD_CODE,
        communities::COMMUNITIES,
        deps::DEPS,
        around::AROUND,
        external_surface::EXTERNAL_SURFACE,
        arch::ARCH,
        mermaid::MERMAID,
        symbol_metrics::SYMBOL_METRICS,
        quality_panel::QUALITY_PANEL,
        field_affinity::FIELD_AFFINITY,
        context_ratio::CONTEXT_RATIO,
        comment_coherence::COMMENT_COHERENCE,
    );
    assert_group(
        &architecture,
        &[
            "--dead-code",
            "--communities",
            "--community=1",
            "--deps",
            "--around=a",
            "--external-surface",
            "--arch=a",
            "--mermaid",
            "--metrics",
            "--quality-panel",
            "--field-affinity",
            "--context-ratio",
            "--comment-coherence",
        ],
        &["--sarif", "--around-depth=2", "--include-builtins"],
    );
}
