use crate::domain::{IngestResult, Interner};
use crate::graph::Graph;
use crate::serialize::{ToXmlString, XmlEscapeExt};
use std::io::{self, Write};
use std::path::Path;

#[derive(Clone, Copy)]
pub struct SafetyContext<'a> {
    pub root: &'a str,
    pub ingest: &'a IngestResult,
    pub graph: &'a Graph,
    pub interner: &'a Interner,
}

#[derive(Default, Clone, Copy)]
pub struct CochangeOptions<'a> {
    pub since: Option<&'a str>,
    pub cochange_recur: Option<u32>,
    pub cochange_groups: bool,
    pub page: crate::serialize::PageRequest,
}
pub fn execute_hotspots_with<W: Write>(
    since_str: Option<&str>,
    commits: &[crate::git::history::CommitInfo],
    state: &SafetyContext<'_>,
    page: crate::serialize::PageRequest,
    out: &mut W,
) -> io::Result<()> {
    let anchor_epoch = commits.first().map(|c| c.timestamp).unwrap_or(0);
    let scope = crate::git::history::parse_since_scope(since_str, anchor_epoch);
    let filtered = crate::git::history::filter_commits_by_scope(commits, &scope);
    let churn_map = crate::git::history::compute_file_churn(
        filtered,
        anchor_epoch,
        crate::git::history::MERGE_BOMB_MAX_FILES,
    );
    let counts: std::collections::HashMap<String, u32> = churn_map
        .into_iter()
        .map(|(path, fc)| (path, fc.commits))
        .collect();
    let report = crate::metrics::compute_hotspots(state.ingest, state.interner, &counts)
        .with_root(state.root)
        .with_page(page);
    report.write_xml(out)
}

pub fn print_cochange_partners<W: Write>(
    file: &str,
    partners: &[crate::git::cochange::CoPartner],
    out: &mut W,
) -> io::Result<()> {
    writeln!(out, "<f path=\"{}\" partners=\"{}\">", file, partners.len())?;
    for p in partners {
        let attr = if !p.dep_capable {
            " dep_capable=\"0\""
        } else if p.surprising {
            " surprising=\"1\""
        } else {
            ""
        };
        writeln!(
            out,
            "  <partner path=\"{}\" together=\"{}\" deg=\"{:.2}\" deg_rev=\"{:.2}\" recur=\"{}\"{} />",
            p.file_path, p.together, p.deg, p.deg_rev, p.recur, attr
        )?;
    }
    writeln!(out, "</f>")
}

fn format_copair_row(p: &crate::git::cochange::CoPair) -> String {
    let surp = usize::from(p.surprising);
    let mut row = format!(
        "  <pair a=\"{}\" b=\"{}\" together=\"{}\" deg=\"{:.2}\" conf_ab=\"{:.2}\" conf_ba=\"{:.2}\" driver=\"{}\" recur=\"{}\" surprising=\"{surp}\"",
        p.file_a.xml_escaped(),
        p.file_b.xml_escaped(),
        p.together,
        p.deg,
        p.conf_ab,
        p.conf_ba,
        p.driver,
        p.recur
    );
    if !p.dep_capable {
        row.push_str(" dep_capable=\"0\"");
    }
    row.push_str("/>");
    row
}

fn format_cogroup_row(g: &crate::git::cochange::CoGroup) -> String {
    let partners_joined = g
        .partners
        .iter()
        .map(|p| p.xml_escaped().to_string())
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "  <group core=\"{}\" partners=\"{partners_joined}\" size=\"{}\" together=\"{}\" deg=\"{:.2}\"/>",
        g.core.xml_escaped(),
        g.partners.len() + 1,
        g.max_together,
        g.max_deg
    )
}

pub fn print_cochange_pairs_paged<W: Write>(
    root: &str,
    pairs: &[crate::git::cochange::CoPair],
    page: crate::serialize::PageRequest,
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::{GitStampExt, Paged};
    let at_attr = root.git_at_xml_attr();
    let total = pairs.len();
    let rows: Vec<String> = pairs.iter().map(format_copair_row).collect();
    let paged = Paged::new(&rows, page, |row| row.len());
    let pw = paged.window;
    let page_attrs = pw.xml_attrs();
    writeln!(
        out,
        "<cochange schema=\"ripwire-rs.cochange/v1\" pairs=\"{total}\" window=\"18mo@HEAD\" sub_windows=\"3\"{page_attrs} root=\"{root}\"{at_attr}>",
    )?;
    for row in paged.rows() {
        writeln!(out, "{row}")?;
    }
    writeln!(out, "</cochange>")
}

pub fn print_cochange_groups_paged<W: Write>(
    root: &str,
    groups: &[crate::git::cochange::CoGroup],
    page: crate::serialize::PageRequest,
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::{GitStampExt, Paged};
    let at_attr = root.git_at_xml_attr();
    let total = groups.len();
    let rows: Vec<String> = groups.iter().map(format_cogroup_row).collect();
    let paged = Paged::new(&rows, page, |row| row.len());
    let pw = paged.window;
    let page_attrs = pw.xml_attrs();
    writeln!(
        out,
        "<cochange-groups schema=\"ripwire-rs.cochange-groups/v1\" groups=\"{total}\" window=\"18mo@HEAD\"{page_attrs} root=\"{root}\"{at_attr}>",
    )?;
    for row in paged.rows() {
        writeln!(out, "{row}")?;
    }
    writeln!(out, "</cochange-groups>")
}

pub fn execute_cochange_with<W: Write>(
    probed_file: Option<&str>,
    since_str: Option<&str>,
    commits: &[crate::git::history::CommitInfo],
    state: &SafetyContext<'_>,
    out: &mut W,
) -> io::Result<()> {
    execute_cochange_with_options(
        probed_file,
        CochangeOptions {
            since: since_str,
            ..Default::default()
        },
        commits,
        state,
        out,
    )
}

pub fn execute_cochange_with_options<W: Write>(
    probed_file: Option<&str>,
    flags: CochangeOptions<'_>,
    commits: &[crate::git::history::CommitInfo],
    state: &SafetyContext<'_>,
    out: &mut W,
) -> io::Result<()> {
    let anchor_epoch = commits.first().map(|c| c.timestamp).unwrap_or(0);
    let scope = crate::git::history::parse_since_scope(flags.since, anchor_epoch);
    let filtered = crate::git::history::filter_commits_by_scope(commits, &scope);
    let coupling = crate::git::cochange::StaticCouplingMap::build(
        state.ingest,
        state.interner,
        Some(state.graph),
    );
    let opts = crate::git::cochange::CochangeOptions::default();

    let target_file = probed_file.filter(|f| {
        !f.is_empty()
            && *f != "."
            && !state.ingest.files.iter().any(|entry| {
                state
                    .interner
                    .resolve(&entry.path)
                    .strip_prefix(f.trim_end_matches('/'))
                    .is_some_and(|suffix| suffix.starts_with('/'))
            })
    });
    if let Some(file) = target_file {
        let mut partners = crate::git::cochange::mine_cochange_for_file(
            file,
            filtered,
            Some(&coupling),
            Some(state.ingest),
            Some(state.interner),
            &opts,
        );
        if let Some(mr) = flags.cochange_recur {
            partners.retain(|p| p.recur >= mr);
        }
        print_cochange_partners(file, &partners, out)
    } else {
        let mut pairs = crate::git::cochange::mine_repo_cochange_pairs(
            filtered,
            Some(&coupling),
            Some(state.ingest),
            Some(state.interner),
            &opts,
        );
        if let Some(mr) = flags.cochange_recur {
            pairs.retain(|p| p.recur >= mr);
        }
        if flags.cochange_groups {
            let groups = crate::git::cochange::cluster_cochange_groups(&pairs);
            print_cochange_groups_paged(state.root, &groups, flags.page, out)
        } else {
            print_cochange_pairs_paged(state.root, &pairs, flags.page, out)
        }
    }
}

pub fn execute_edit_check_with<W: Write>(
    sym_name: &str,
    page: Option<(usize, usize)>,
    state: &SafetyContext<'_>,
    out: &mut W,
) -> io::Result<()> {
    let ids = crate::navigate::find_symbols_by_name(sym_name, state.ingest, state.interner);
    let Some(&sym_id) = ids.first() else {
        return Err(io::Error::other(format!("Symbol '{sym_name}' not found")));
    };

    let sym = &state.ingest.symbols[sym_id as usize];
    let rel_path = state
        .interner
        .resolve(&state.ingest.files[sym.file_id as usize].path);
    let head_contract =
        crate::gate::contract::load_head_symbol_contract(state.root, rel_path, sym_name, sym.lang)
            .ok()
            .flatten();

    write_edit_check_from_facts(
        &EditCheckRequest {
            sym_name,
            head_contract: head_contract.as_ref(),
            page,
            at_stamp: crate::git::history::compute_git_at_stamp(state.root).as_deref(),
        },
        state,
        out,
    )
}

#[derive(Default, Clone, Copy)]
pub struct EditCheckRequest<'a> {
    pub sym_name: &'a str,
    pub head_contract: Option<&'a crate::gate::contract::SignatureContract>,
    pub page: Option<(usize, usize)>,
    pub at_stamp: Option<&'a str>,
}

pub fn write_edit_check_from_facts<W: Write>(
    req: &EditCheckRequest<'_>,
    state: &SafetyContext<'_>,
    out: &mut W,
) -> io::Result<()> {
    let ids = crate::navigate::find_symbols_by_name(req.sym_name, state.ingest, state.interner);
    let Some(&sym_id) = ids.first() else {
        return Err(io::Error::other(format!(
            "Symbol '{}' not found",
            req.sym_name
        )));
    };
    if let Some(mut report) = crate::gate::edit_check::check_symbol_contract(
        sym_id,
        req.head_contract,
        state.ingest,
        state.graph,
        state.interner,
    ) {
        if let Some(stamp) = req.at_stamp {
            report.at_stamp = stamp.to_string();
        }
        report.page = req.page;
        report.write_xml(out)?;
    }
    Ok(())
}

pub fn execute_affected_with<W: Write>(
    specs_str: &str,
    state: &SafetyContext<'_>,
    out: &mut W,
) -> io::Result<()> {
    affected_from_sources(
        specs_str,
        state,
        &crate::query::FileSources {
            root: Path::new(state.root),
        },
    )
    .write_xml(out)
}

pub fn affected_from_sources(
    specs_str: &str,
    state: &SafetyContext<'_>,
    sources: &(impl crate::query::SourceReader + ?Sized),
) -> crate::gate::affected::AffectedReport {
    let specs: Vec<&str> = specs_str.split(',').map(str::trim).collect();
    let mut report = crate::gate::affected::find_affected_tests(
        &specs,
        state.ingest,
        state.graph,
        state.interner,
    );
    report.script_gates_unmodelled =
        crate::gate::script_gates::collect_script_gates(state.root, state.ingest, state.interner)
            .unmodelled;
    crate::gate::runners::attach_test_runners_from_sources(
        &mut report.tests,
        sources,
        state.ingest,
        state.interner,
    );
    report
}
pub fn dirty_pin_absorbed_gating(
    state: &crate::query::commands::QueryContext<'_>,
) -> io::Result<Option<(usize, String)>> {
    if crate::git::repo::working_tree_changes(state.root).is_empty() {
        return Ok(None);
    }
    let rep = crate::gate::quality_delta::evaluate_quality_delta_with_basis(
        state.root,
        state,
        crate::gate::quality_delta::DeltaBasis::HeadOnly,
    )?;
    let mut gating = rep.regressions.iter().filter(|r| r.is_gating);
    let Some(first) = gating.next() else {
        return Ok(None);
    };
    let first_row = format!(
        "{} {} {}:{} was={} now={}",
        first.kind, first.sym_name, first.file_path, first.line, first.was_val, first.now_val
    );
    Ok(Some((1 + gating.count(), first_row)))
}

fn write_baseline_confirmation<W: Write>(
    head_sha: &str,
    baseline_path: &Path,
    out: &mut W,
) -> io::Result<()> {
    let stamp_label = if head_sha.is_empty() {
        "no git HEAD"
    } else {
        head_sha
    };
    writeln!(
        out,
        "ripwire-rs: pinned quality baseline stamped at {stamp_label} -> {}",
        baseline_path.display()
    )
}

fn prepare_quality_baseline(
    state: &crate::query::commands::QueryContext<'_>,
    head_sha: &str,
    allow_dirty: bool,
) -> io::Result<crate::gate::baseline::QualitySnapshot> {
    let absorbed = dirty_pin_absorbed_gating(state)?;
    let absorbed_ref = absorbed.as_ref().map(|(count, row)| (*count, row.as_str()));
    baseline_from_facts(
        &SafetyContext {
            root: state.root,
            ingest: state.ingest,
            graph: state.graph,
            interner: state.interner,
        },
        head_sha,
        absorbed_ref,
        allow_dirty,
    )
}

pub fn execute_quality_baseline_with<W: Write>(
    state: &crate::query::commands::QueryContext<'_>,
    allow_dirty: bool,
    out: &mut W,
) -> io::Result<()> {
    let head_sha = crate::git::history::resolve_head_sha(state.root).unwrap_or_default();
    let snap = prepare_quality_baseline(state, &head_sha, allow_dirty)?;
    let baseline_path = Path::new(state.root).join(crate::gate::baseline::DEFAULT_BASELINE_FILE);
    crate::gate::baseline::write_baseline_file(&snap, &baseline_path)?;
    write_baseline_confirmation(&head_sha, &baseline_path, out)
}
/// Runs the delta against the pinned sidecar or HEAD, or the merge base of `args.base` when given.
pub fn execute_quality_delta_with_args<W: Write>(
    args: crate::gate::quality_delta::DeltaCliOptions<'_>,
    state: &crate::query::commands::QueryContext<'_>,
    out: &mut W,
) -> io::Result<bool> {
    let mut report = crate::gate::quality_delta::evaluate_quality_delta_from(state, args.base)?;
    crate::gate::quality_delta::apply_cli_delta_options(state, &mut report, args)?;
    report.write_xml(out)?;
    Ok(report.has_gating_failures())
}

pub fn baseline_from_facts(
    state: &SafetyContext<'_>,
    head_sha: &str,
    absorbed: Option<(usize, &str)>,
    allow_dirty: bool,
) -> io::Result<crate::gate::baseline::QualitySnapshot> {
    let mut snap = crate::gate::baseline::compute_snapshot(
        state.ingest,
        state.graph,
        state.interner,
        head_sha,
    );
    if let Some((absorbed, first_row)) = absorbed {
        if !allow_dirty {
            return Err(io::Error::other(format!(
                "ripwire-rs: --quality-baseline: this tree already holds {absorbed} gating finding(s) against HEAD; pinning here would absorb them into the floor. First: {first_row}\n  Commit the tree first, or pass --allow-dirty to pin anyway (the sidecar then records absorbed={absorbed})."
            )));
        }
        snap.dirty = true;
        snap.absorbed = absorbed;
    }
    Ok(snap)
}

pub struct SituationalReport<'a> {
    pub root: &'a str,
    pub files: &'a [String],
    pub commits: Option<&'a [crate::git::history::CommitInfo]>,
    pub page: crate::serialize::PageRequest,
    pub context: SafetyContext<'a>,
}

impl crate::serialize::XmlReport for SituationalReport<'_> {
    crate::xml_report_meta!("situ", "ripwire-rs.situ/v1", root: true);
    fn root_path(&self) -> &str {
        self.root
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let facts = crate::query::graph::call_situational_awareness(
            &self.context,
            self.files,
            self.commits,
            self.page,
        );
        writeln!(
            out,
            "  <summary files=\"{}\" blast_radius=\"{}\"/>",
            self.files.len(),
            facts["blast_radius"]
        )?;
        for (key, tag) in [
            ("files_touched", "changed"),
            ("tests_to_run", "test"),
            ("forgotten_partners", "partner"),
        ] {
            if let Some(rows) = facts[key].as_array() {
                for row in rows {
                    writeln!(
                        out,
                        "  <{tag} path=\"{}\"/>",
                        row.as_str().unwrap_or("").xml_escaped()
                    )?;
                }
            }
        }
        Ok(())
    }
}
