use ripwire_rs::domain::{RefRole, ScipSite, ScipTarget};
use ripwire_rs::metrics::scip_eval::{
    ScipEvalReport, SiteVerdict, evaluate_against_scip, serialize_scip_eval_xml,
    serialize_scip_verdicts_xml,
};
use ripwire_rs::resolve::RefOutcome;
use ripwire_rs::{
    EdgeProv, IngestResult, Interner, Lang, LanguageRegistry, ResolvedRef, ingest_in_memory_files,
};
use std::collections::HashSet;

fn resolved(targets: Vec<u32>) -> RefOutcome {
    RefOutcome::Resolved(ResolvedRef {
        from: 0,
        targets,
        conf_per_target: 1.0,
        prov: EdgeProv::Local,
        type_use: false,
    })
}

#[test]
fn verdicts_separate_exact_split_wrong_missed_and_external_cases() {
    let def = ScipTarget::Def(7);
    assert_eq!(
        SiteVerdict::verdict(def, &resolved(vec![7])),
        SiteVerdict::Exact
    );
    assert_eq!(
        SiteVerdict::verdict(def, &resolved(vec![3, 7])),
        SiteVerdict::SplitHit
    );
    assert_eq!(
        SiteVerdict::verdict(def, &resolved(vec![3])),
        SiteVerdict::Wrong
    );
    assert_eq!(
        SiteVerdict::verdict(def, &RefOutcome::Unresolved),
        SiteVerdict::Missed
    );
    assert_eq!(
        SiteVerdict::verdict(def, &RefOutcome::External),
        SiteVerdict::Missed
    );
    let ext = ScipTarget::External;
    assert_eq!(
        SiteVerdict::verdict(ext, &resolved(vec![7])),
        SiteVerdict::FalseEdge
    );
    assert_eq!(
        SiteVerdict::verdict(ext, &RefOutcome::External),
        SiteVerdict::ExternalAgreed
    );
    assert_eq!(
        SiteVerdict::verdict(ext, &RefOutcome::Declined),
        SiteVerdict::ExternalAgreed
    );
}

/// `helper()` is called on lines 5 and 6 of `app.py`; the index pins line 5 to the definition
/// and line 6 to an external symbol. The `print(1)` call on line 7 is never pinned.
struct Fixture {
    ingest: IngestResult,
    interner: Interner,
    sites: [ScipSite; 2],
}

fn fixture() -> Fixture {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[(
            "app.py",
            Lang::Python,
            b"def helper():\n    return 1\n\ndef entry():\n    helper()\n    helper()\n    print(1)\n"
                .as_slice(),
        )],
        &registry,
        &interner,
    );
    let helper = ingest
        .symbols
        .iter()
        .find(|s| interner.resolve(&s.name) == "helper")
        .unwrap()
        .id;
    let site = |line: u32, target| {
        let (file_id, callee) = call_at(&ingest, line);
        ScipSite {
            file_id,
            line,
            callee,
            target,
        }
    };
    let sites = [
        site(5, ScipTarget::Def(helper)),
        site(6, ScipTarget::External),
    ];
    Fixture {
        ingest,
        interner,
        sites,
    }
}

fn call_at(ingest: &IngestResult, line: u32) -> (u32, lasso::Spur) {
    ingest
        .references
        .iter()
        .find(|r| r.role == RefRole::Call && r.name_line == line)
        .map(|r| (r.file_id, r.callee_name))
        .unwrap()
}

fn documented_app(f: &Fixture) -> HashSet<u32> {
    HashSet::from([call_at(&f.ingest, 5).0])
}

#[test]
fn only_pinned_sites_are_scored_and_rates_follow_the_verdicts() {
    let f = fixture();
    let unjudged = evaluate_against_scip(&f.ingest, &f.interner, &f.sites, &Default::default());
    assert_eq!(
        unjudged.count(SiteVerdict::Unindexed),
        1,
        "an edge into a file the index never saw is not a false edge"
    );
    assert_eq!(unjudged.precision(), Some(1.0));
    let report = evaluate_against_scip(&f.ingest, &f.interner, &f.sites, &documented_app(&f));
    assert_eq!(report.covered, 2, "the unpinned print() call is not scored");
    assert!(report.calls >= 3);
    assert_eq!(report.count(SiteVerdict::Exact), 1);
    assert_eq!(report.count(SiteVerdict::FalseEdge), 1);
    assert_eq!(report.precision(), Some(0.5));
    assert_eq!(report.recall(), Some(1.0));

    let mut out = Vec::new();
    serialize_scip_eval_xml(&report, &f.ingest, &f.interner, 10, &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    let doc = roxmltree::Document::parse(&text).unwrap();
    let root = doc.root_element();
    assert_eq!(root.attribute("exact"), Some("1"));
    assert_eq!(root.attribute("false-edge"), Some("1"));
    assert_eq!(root.attribute("precision"), Some("0.5000"));
    let samples: Vec<_> = root.children().filter(|n| n.has_tag_name("site")).collect();
    assert_eq!(
        samples.len(),
        1,
        "only disagreements are sampled, not the exact site"
    );
    let sample = samples[0];
    assert_eq!(sample.attribute("v"), Some("false-edge"));
    assert_eq!(sample.attribute("p"), Some("app.py:6"));
    assert_eq!(
        sample.attribute("scip"),
        None,
        "an external target names no in-tree symbol"
    );
    assert_eq!(sample.attribute("ours"), Some("helper@app.py:1"));
}

#[test]
fn verdict_rows_cover_every_pinned_site_including_agreements() {
    let f = fixture();
    let report: ScipEvalReport =
        evaluate_against_scip(&f.ingest, &f.interner, &f.sites, &documented_app(&f));
    let mut verdicts = Vec::new();
    serialize_scip_verdicts_xml(&report, &f.ingest, &f.interner, &mut verdicts).unwrap();
    let text = String::from_utf8(verdicts).unwrap();
    let doc = roxmltree::Document::parse(&text).unwrap();
    let rows: Vec<_> = doc
        .root_element()
        .children()
        .filter(|n| n.has_tag_name("verdict"))
        .collect();
    assert_eq!(
        rows.len(),
        report.covered,
        "one verdict per covered site, agreements included"
    );
    assert_eq!(
        rows.iter()
            .filter(|n| n.attribute("v") == Some("exact") && n.attribute("p") == Some("app.py:5"))
            .count(),
        1
    );
    assert!(rows.iter().any(|n| n.attribute("v") == Some("false-edge")
        && n.attribute("p") == Some("app.py:6")
        && n.attribute("callee") == Some("helper")));
}
