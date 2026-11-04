use super::{DeclinedCall, ImpactDef, ImpactNode, NavNeighborhood, ReachFile, SymbolNavInfo};
use crate::domain::{IngestResult, Interner, SymKind};
use crate::serialize::{Paged, XmlEscapeExt, row_bytes};
use std::io::{self, Write};

/// Direction of a 1-hop navigation report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavDir {
    Callers,
    Callees,
}

impl NavDir {
    fn tag(self) -> &'static str {
        match self {
            Self::Callers => "callers",
            Self::Callees => "callees",
        }
    }
}

pub struct NavItemsReport<'a> {
    pub dir: NavDir,
    pub root: &'a str,
    pub of: &'a str,
    pub defs: usize,
    pub def_rows: &'a [ImpactDef<'a>],
    pub items: &'a [SymbolNavInfo],
    pub declined: &'a [DeclinedCall<'a>],
    pub gauge_attrs: &'a str,
    pub page: crate::serialize::PageRequest,
}

impl NavItemsReport<'_> {
    fn paged_items(&self) -> Paged<'_, SymbolNavInfo> {
        Paged::new(self.items, self.page, |it| {
            row_bytes(|mut w| self.write_item(&mut w, it))
        })
    }

    fn grouped(&self) -> bool {
        self.def_rows.len() >= 2
    }

    fn of_def_attr(&self, item: &SymbolNavInfo) -> String {
        if !self.grouped() {
            return String::new();
        }
        let ixs: Vec<String> = self
            .def_rows
            .iter()
            .enumerate()
            .filter_map(|(ix, d)| {
                let (_, prov) = item.of_defs.iter().find(|(id, _)| d.members.contains(id))?;
                Some(format!("{}{}", ix + 1, prov.of_def_suffix()))
            })
            .collect();
        format!(" of_def=\"{}\"", ixs.join(" "))
    }

    fn of_all_defs_attr(&self) -> String {
        if !self.grouped() {
            return String::new();
        }
        let ixs: Vec<String> = (1..=self.def_rows.len())
            .map(|ix| format!("{ix}~d"))
            .collect();
        format!(" of_def=\"{}\"", ixs.join(" "))
    }

    fn declined_attr(&self) -> String {
        match self.declined.len() {
            0 => String::new(),
            n => format!(" declined_calls=\"{n}\""),
        }
    }

    /// `role=`, `prov=`, and `via=` for one row: how the edge to or from it bound.
    fn binding_attrs(&self, it: &SymbolNavInfo) -> String {
        let prov = match self.dir {
            NavDir::Callees => it.prov,
            NavDir::Callers => it.of_defs.iter().map(|(_, p)| *p).max().unwrap_or(it.prov),
        };
        let role = if self.dir == NavDir::Callees && it.kind == SymKind::Macro.tag() {
            " role=\"macro\""
        } else {
            ""
        };
        match (self.dir, prov.callee_label()) {
            (NavDir::Callees, Some(label)) => format!("{role} prov=\"{label}\""),
            _ if prov.is_by_name() => format!("{role} via=\"name\""),
            _ => role.to_string(),
        }
    }

    fn write_item<W: Write>(&self, out: &mut W, it: &SymbolNavInfo) -> io::Result<()> {
        let binding_attrs = self.binding_attrs(it);
        let tested_attr = if it.tested || crate::domain::test_path::is_test_path(&it.path) {
            " tested=\"1\""
        } else {
            ""
        };
        let sites_attr = if self.dir == NavDir::Callers && !it.sites.is_empty() {
            format!(" sites_at=\"{}\"", it.sites.join(" ").xml_escaped())
        } else {
            String::new()
        };
        write!(
            out,
            "<s t=\"{}\" n=\"{}\" p=\"{}:{}\"{binding_attrs}{}{tested_attr}{sites_attr}/>",
            it.kind,
            it.name.xml_escaped(),
            it.path.xml_escaped(),
            it.line,
            self.of_def_attr(it)
        )
    }

    fn write_rows<W: Write>(
        &self,
        items: &Paged<'_, SymbolNavInfo>,
        out: &mut W,
    ) -> io::Result<()> {
        for it in items.rows() {
            self.write_item(out, it)?;
        }
        let all_defs = self.of_all_defs_attr();
        for dc in self.declined {
            write_declined_row(out, dc, Some(self.dir), &all_defs)?;
        }
        Ok(())
    }

    fn write_attrs<W: Write>(
        &self,
        items: &Paged<'_, SymbolNavInfo>,
        out: &mut W,
    ) -> io::Result<()> {
        let win = items.window;
        let count = self.items.len();
        let hop_tested = self
            .items
            .iter()
            .filter(|it| it.tested || crate::domain::test_path::is_test_path(&it.path))
            .count();
        let hop_untested = count.saturating_sub(hop_tested);
        let of = self.of.xml_escaped();
        let tag = self.dir.tag();
        let uses_next = format!(" uses_next=\"--uses={of}\"");
        let next = match (win.has_more, self.dir) {
            (true, dir) => format!(
                " next=\"--{tag}={of} --offset={}\"{}",
                win.next_offset,
                if dir == NavDir::Callers {
                    uses_next.as_str()
                } else {
                    ""
                }
            ),
            (false, NavDir::Callers) => format!(" next=\"--uses={of}\""),
            (false, NavDir::Callees) => format!(" next=\"--expand={of}\""),
        };
        write!(
            out,
            " of=\"{of}\" defs=\"{}\" count=\"{count}\" root=\"{}\" hop_tested=\"{hop_tested}\" hop_untested=\"{hop_untested}\"{}{} {}{next}",
            self.defs,
            self.root.xml_escaped(),
            self.declined_attr(),
            win.xml_attrs(),
            self.gauge_attrs
        )
    }
}

fn write_def_rows<W: Write>(out: &mut W, defs: &[ImpactDef<'_>]) -> io::Result<()> {
    for (ix, d) in defs.iter().enumerate() {
        let sc = d
            .scope
            .map(|s| format!(" sc=\"{}\"", s.xml_escaped()))
            .unwrap_or_default();
        write!(
            out,
            "<def ix=\"{}\" t=\"{}\"{sc} p=\"{}:{}\"{}/>",
            ix + 1,
            d.kind,
            d.path.xml_escaped(),
            d.line,
            if d.bodyless { " bodyless=\"1\"" } else { "" }
        )?;
    }
    Ok(())
}

fn write_declined_row<W: Write>(
    out: &mut W,
    dc: &DeclinedCall<'_>,
    dir: Option<NavDir>,
    of_def: &str,
) -> io::Result<()> {
    match dir {
        Some(NavDir::Callees) => write!(
            out,
            "<dc n=\"{}\" l=\"{}\" cands=\"{}\" via=\"declined\"/>",
            dc.callee.xml_escaped(),
            dc.line,
            dc.cands
        ),
        _ => write!(
            out,
            "<dc in=\"{}\" p=\"{}:{}\" cands=\"{}\" via=\"declined\"{of_def}/>",
            dc.enclosing.xml_escaped(),
            dc.path.xml_escaped(),
            dc.line,
            dc.cands
        ),
    }
}

impl crate::serialize::XmlReport for NavItemsReport<'_> {
    fn schema_tag(&self) -> &'static str {
        self.dir.tag()
    }
    fn schema_version(&self) -> &'static str {
        match self.dir {
            NavDir::Callers => "ripwire-rs.callers/v1",
            NavDir::Callees => "ripwire-rs.callees/v1",
        }
    }
    fn include_root_attr(&self) -> bool {
        false
    }
    fn legend_comment(&self) -> Option<&'static str> {
        Some(match self.dir {
            NavDir::Callers => {
                "ripwire-rs callers schema=ripwire-rs.callers/v1: 1-hop callers of of=. <def ix= t= sc= p= bodyless=1>: the groups defs= forms (a definition and the declarations tied to it; bodyless=1 tied to none), listed when there are two or more. of_def=K[~s|~t|~n]: the group the caller edge reaches (~s: one arm of an ambiguous call; ~t: a primary template whose specialization is another group; ~n: bound by name alone). via=name: the edge bound by name alone. declined_calls=K: call sites naming of= the resolver left unbound, in no count; <dc in= p= cands= via=declined>: one such site (calling def, file:line), NOT an edge; cands=K: in-tree defs it could have meant."
            }
            NavDir::Callees => {
                "ripwire-rs callees schema=ripwire-rs.callees/v1: 1-hop callees of of=. <def ix= t= sc= p= bodyless=1> lists definition groups when there are two or more; of_def= identifies which groups call each callee, with ~s|~t|~n split/template/name provenance. role=macro: the callee is a macro expansion. via=name: bound by name alone (same directory or repository-wide). prov=scip|binding|import|final-segment|split: how the edge bound; scip = pinned by a compiler index; binding = exposed to the caller's language by a binding framework (pybind11 .def); import = through a module alias the file imports; final-segment = a type qualifier matched by last name only; split = one arm of an ambiguous call. declined_calls=K: call sites inside of= the resolver left unbound, in no count; <dc n= l= cands= via=declined>: one such site (called name, line), NOT an edge; cands=K: in-tree defs it could have meant."
            }
        })
    }
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        self.write_attrs(&self.paged_items(), out)
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        if self.grouped() {
            write_def_rows(out, self.def_rows)?;
        }
        self.write_rows(&self.paged_items(), out)
    }
}

pub struct ImpactReport<'a> {
    pub root: &'a str,
    pub sym_name: &'a str,
    pub defs: Vec<ImpactDef<'a>>,
    pub nodes: &'a [ImpactNode],
    pub importers: Vec<&'a str>,
    pub declined: Vec<DeclinedCall<'a>>,
    pub gauge_attrs: &'a str,
    pub pr_iters: Option<u32>,
    pub page: crate::serialize::PageRequest,
}

impl ImpactReport<'_> {
    fn reach(&self) -> Paged<'_, ImpactNode> {
        Paged::new(self.nodes, self.page, |n| {
            row_bytes(|mut w| write_reach_row(&mut w, n, true))
        })
    }

    fn importers(&self) -> Paged<'_, &str> {
        Paged::new(&self.importers, self.page.nested(), |p| {
            row_bytes(|mut w| write_importer_row(&mut w, p))
        })
    }

    fn declined(&self) -> Paged<'_, DeclinedCall<'_>> {
        Paged::new(&self.declined, self.page.nested(), |dc| {
            row_bytes(|mut w| write_declined_row(&mut w, dc, None, ""))
        })
    }

    fn by_depth_attr(&self) -> String {
        let mut depth_counts: std::collections::BTreeMap<u32, usize> =
            std::collections::BTreeMap::new();
        for n in self.nodes.iter().filter(|n| !n.by_name) {
            *depth_counts.entry(n.hops).or_insert(0) += 1;
        }
        if depth_counts.is_empty() {
            return String::new();
        }
        let by_depth = depth_counts
            .iter()
            .map(|(d, c)| format!("{d}:{c}"))
            .collect::<Vec<_>>()
            .join(",");
        format!(" by_depth=\"{by_depth}\"")
    }

    fn importer_attrs(&self) -> String {
        let importers = self.importers();
        let win = importers.window;
        format!(
            " importers=\"{}\" shown_importers=\"{}\" importers_capped=\"{}\"",
            win.total,
            win.shown,
            usize::from(win.capped)
        )
    }

    fn declined_attrs(&self) -> String {
        let declined = self.declined();
        let win = declined.window;
        if win.total == 0 {
            return String::new();
        }
        if !win.capped {
            return format!(" declined_calls=\"{}\"", win.total);
        }
        format!(
            " declined_calls=\"{}\" dc_shown=\"{}\"",
            win.total, win.shown
        )
    }

    fn write_files_block<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let files = super::reach_files(self.nodes);
        let page = Paged::new(&files, self.page.nested(), |f| {
            row_bytes(|mut w| write_reach_file(&mut w, f))
        });
        let win = page.window;
        write!(
            out,
            "<files files=\"{}\" shown_files=\"{}\" files_capped=\"{}\">",
            win.total,
            win.shown,
            usize::from(win.capped)
        )?;
        for f in page.rows() {
            write_reach_file(out, f)?;
        }
        write!(out, "</files>")
    }
}

fn write_reach_row<W: Write>(out: &mut W, n: &ImpactNode, show_depth: bool) -> io::Result<()> {
    let depth_attr = if show_depth {
        format!(" d=\"{}\"", n.hops)
    } else {
        String::new()
    };
    let tested_attr = if n.tested || crate::domain::test_path::is_test_path(&n.path) {
        " tested=\"1\""
    } else {
        ""
    };
    let via_attr = if n.by_name { " via=\"name\"" } else { "" };
    write!(
        out,
        "<s t=\"{}\" n=\"{}\" p=\"{}:{}\"{depth_attr}{tested_attr}{via_attr}/>",
        n.kind,
        n.name.xml_escaped(),
        n.path.xml_escaped(),
        n.line
    )
}

fn write_reach_file<W: Write>(out: &mut W, f: &ReachFile<'_>) -> io::Result<()> {
    write!(
        out,
        "<rf p=\"{}\" syms=\"{}\" d=\"{}\"/>",
        f.path.xml_escaped(),
        f.syms,
        f.hops
    )
}

fn write_importer_row<W: Write>(out: &mut W, p: &str) -> io::Result<()> {
    write!(
        out,
        "<f via=\"import\" p=\"{}\" lazy=\"0\"/>",
        p.xml_escaped()
    )
}

impl crate::serialize::XmlReport for ImpactReport<'_> {
    crate::xml_report_meta!(
        "impact",
        "ripwire-rs.impact/v1",
        root: false,
        comment: "ripwire-rs impact schema=ripwire-rs.impact/v1: transitive blast radius of of=. via=name on <s>: reached only through an edge bound by name alone, a hedge not a proven caller; reaches=, by_depth= and radius_* count proven rows only and hedged=K counts the rest. declined_calls=K: call sites naming a reached def the resolver left unbound; <dc in= p= cands=>: one such site, NOT an edge; dc_shown=N rows printed (file-first)."
    );
    fn pr_iters(&self) -> Option<u32> {
        self.pr_iters
    }
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let proven = self.nodes.iter().filter(|n| !n.by_name);
        let reaches = proven.clone().count();
        let hedged = self.nodes.len() - reaches;
        let hedged_attr = if hedged > 0 {
            format!(" hedged=\"{hedged}\"")
        } else {
            String::new()
        };
        let (radius_tested, radius_untested) = super::radius_split(proven);
        let of = self.sym_name.xml_escaped();
        write!(
            out,
            " of=\"{of}\" defs=\"{}\" reaches=\"{reaches}\"{hedged_attr}{}{} radius_tested=\"{radius_tested}\" radius_untested=\"{radius_untested}\"{} root=\"{}\"{} {} next=\"--safe-delete={of}\"",
            self.defs.iter().map(|d| d.members.len()).sum::<usize>(),
            self.by_depth_attr(),
            self.importer_attrs(),
            self.declined_attrs(),
            self.root.xml_escaped(),
            self.reach().attrs(),
            self.gauge_attrs,
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        if self.defs.len() >= 2 {
            write_def_rows(out, &self.defs)?;
        }
        let reach = self.reach();
        if reach.window.capped {
            self.write_files_block(out)?;
        }
        let mut prev_hops = None;
        for n in reach.rows() {
            write_reach_row(out, n, prev_hops != Some(n.hops))?;
            prev_hops = Some(n.hops);
        }
        for p in self.importers().rows() {
            write_importer_row(out, p)?;
        }
        for dc in self.declined().rows() {
            write_declined_row(out, dc, None, "")?;
        }
        Ok(())
    }
}

/// Definition groups and their incoming/outgoing neighborhoods under one query root.
pub struct FindSymbolReport<'a> {
    pub root: &'a str,
    pub of: &'a str,
    pub defs: usize,
    pub def_rows: &'a [ImpactDef<'a>],
    pub nav: &'a NavNeighborhood,
    pub callers_declined: &'a [DeclinedCall<'a>],
    pub callees_declined: &'a [DeclinedCall<'a>],
    pub page: crate::serialize::PageRequest,
}

impl FindSymbolReport<'_> {
    fn neighbors(&self, dir: NavDir) -> NavItemsReport<'_> {
        let (items, declined) = match dir {
            NavDir::Callers => (&self.nav.callers, self.callers_declined),
            NavDir::Callees => (&self.nav.callees, self.callees_declined),
        };
        NavItemsReport {
            dir,
            root: self.root,
            of: self.of,
            defs: self.defs,
            def_rows: self.def_rows,
            items,
            declined,
            gauge_attrs: &self.nav.gauge_attrs,
            page: self.page,
        }
    }
}

impl crate::serialize::XmlReport for FindSymbolReport<'_> {
    crate::xml_report_meta!(
        "find-symbol",
        "ripwire-rs.find-symbol/v1",
        root: true,
        comment: "ripwire-rs find-symbol: all matching definition groups and their unioned neighborhoods. p= is the primary match; <def ix= t= sc= p= bodyless=1> lists every group, including tied declarations. Inside callers, of_def= identifies the called groups; inside callees, it identifies the calling groups. ~s|~t|~n retain split/template/name binding provenance. prov= and role= describe callee bindings and macros. Each neighborhood has its own count/page and declined sites, which are not edges."
    );
    fn root_path(&self) -> &str {
        self.root
    }
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        write!(
            out,
            " of=\"{}\" p=\"{}:{}\" defs=\"{}\" groups=\"{}\" callers=\"{}\" callees=\"{}\" {}",
            self.of.xml_escaped(),
            self.nav.target.path.xml_escaped(),
            self.nav.target.line,
            self.defs,
            self.def_rows.len(),
            self.nav.callers.len(),
            self.nav.callees.len(),
            self.nav.gauge_attrs,
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        write_def_rows(out, self.def_rows)?;
        for dir in [NavDir::Callers, NavDir::Callees] {
            let report = self.neighbors(dir);
            let items = report.paged_items();
            let tag = dir.tag();
            write!(out, "<{tag}")?;
            report.write_attrs(&items, out)?;
            writeln!(out, ">")?;
            report.write_rows(&items, out)?;
            writeln!(out, "</{tag}>")?;
        }
        Ok(())
    }
}

/// Serialize static use-sites to Ripwire-compatible XML.
pub fn serialize_uses_xml<W: Write>(
    sym_name: &str,
    ingest: &IngestResult,
    interner: &Interner,
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlReport;
    UsesReport::collect(sym_name, ingest, interner).render_envelope(out)
}

pub struct UsesReport<'a> {
    pub root: &'a str,
    pub sym_name: &'a str,
    pub defs: usize,
    pub sites: Vec<(&'static str, &'a str, u32, Option<&'a str>)>,
    pub gauge_attrs: &'a str,
    pub page: crate::serialize::PageRequest,
}

impl<'a> UsesReport<'a> {
    #[must_use]
    pub fn collect(sym_name: &'a str, ingest: &'a IngestResult, interner: &'a Interner) -> Self {
        use crate::domain::CorpusView;
        let mut sites = (ingest, interner).collect_symbol_uses(sym_name);
        sites.sort_by(|a, b| (a.1, a.2).cmp(&(b.1, b.2)));
        Self {
            root: ".",
            sym_name,
            defs: super::find_symbols_by_name(sym_name, ingest, interner).len(),
            sites,
            gauge_attrs: crate::graph::GraphGauges::ZERO_XML_ATTRS,
            page: crate::serialize::PageRequest::default(),
        }
    }

    fn paged_sites(&self) -> Paged<'_, (&'static str, &'a str, u32, Option<&'a str>)> {
        Paged::new(&self.sites, self.page, |site| {
            row_bytes(|mut w| write_use_row(&mut w, site))
        })
    }
}

fn write_use_row<W: Write>(
    out: &mut W,
    &(role, file, line, enc): &(&str, &str, u32, Option<&str>),
) -> io::Result<()> {
    let in_id = enc
        .map(|e| format!(" in_id=\"{}\"", e.xml_escaped()))
        .unwrap_or_default();
    writeln!(
        out,
        "  <u role=\"{role}\" p=\"{}:{line}\"{in_id}/>",
        file.xml_escaped()
    )
}

impl crate::serialize::XmlReport for UsesReport<'_> {
    crate::xml_report_meta!(
        "uses",
        "ripwire-rs.uses/v1",
        root: false,
        comment: "ripwire-rs uses schema=ripwire-rs.uses/v1: resolvable use-sites of of=. defs=N: definitions the selector matched. external=1: of= has no indexed definition. limit=/offset= page the rows; next_offset= continues."
    );
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let sites = self.paged_sites();
        let win = sites.window;
        let page_attrs = if win.capped || win.limit > 0 || win.offset > 0 {
            sites.attrs()
        } else {
            String::new()
        };
        write!(
            out,
            " of=\"{}\" defs=\"{}\" external=\"{}\" count=\"{}\" root=\"{}\"{page_attrs} {}",
            self.sym_name.xml_escaped(),
            self.defs,
            usize::from(self.defs == 0),
            win.total,
            self.root.xml_escaped(),
            self.gauge_attrs
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        for site in self.paged_sites().rows() {
            write_use_row(out, site)?;
        }
        Ok(())
    }
}
