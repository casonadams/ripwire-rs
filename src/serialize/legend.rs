use crate::serialize::escape::XmlEscapeExt;
use std::io::{self, Write};

pub const LEGEND_DICT_VERSION: &str = "80b7e589253a8e76";

pub const LEGEND_DICT_ENTRIES: &[(&str, &str, &str)] = &[
    (
        "schema",
        "ripwire-rs.map/v1",
        "Ranked symbol call-graph map (<r>, <f>, <s>, <c>)",
    ),
    (
        "schema",
        "ripwire-rs.metrics/v1",
        "Ranked map annotated with fan-in, fan-out, complexity, and change amplification",
    ),
    (
        "schema",
        "ripwire-rs.map-diff/v1",
        "Ranked map re-weighted toward git working-tree changed files",
    ),
    (
        "schema",
        "ripwire-rs.for/v1",
        "Task-lens BM25 + PageRank ranked context (<ctx>, <sigs>, <bodies>)",
    ),
    (
        "schema",
        "ripwire-rs.pack-task/v1",
        "Single-call task bundle joining ranked bodies, callers, and affected tests",
    ),
    (
        "schema",
        "ripwire-rs.callers/v1",
        "Direct 1-hop incoming callers of a target symbol",
    ),
    (
        "schema",
        "ripwire-rs.callees/v1",
        "Direct 1-hop outgoing callees of a target symbol",
    ),
    (
        "schema",
        "ripwire-rs.impact/v1",
        "Transitive reverse-call blast radius ordered by hop distance and PageRank",
    ),
    (
        "schema",
        "ripwire-rs.uses/v1",
        "Statically resolvable reference sites (call, read, write, import, extends, type)",
    ),
    (
        "schema",
        "ripwire-rs.find-symbol/v1",
        "1-hop symbol neighborhood containing definitions, callers, and callees",
    ),
    (
        "schema",
        "ripwire-rs.around/v1",
        "Bidirectional ego-graph neighborhood within depth and fanout bounds",
    ),
    (
        "schema",
        "ripwire-rs.connect/v1",
        "Minimal connecting call subgraph joining 2..16 symbols",
    ),
    (
        "schema",
        "ripwire-rs.defs/v1",
        "Definitions bound by 1..16 names (<d>), with names that bind none (<nf>); sel= absent means n= alone pins the definition",
    ),
    (
        "schema",
        "ripwire-rs.path/v1",
        "Shortest directed call path between two symbols",
    ),
    (
        "schema",
        "ripwire-rs.slice/v1",
        "ARISE intra-procedural def-use variable dataflow slice",
    ),
    (
        "schema",
        "ripwire-rs.expand/v1",
        "Full source bodies and callee signatures for requested symbols",
    ),
    (
        "schema",
        "ripwire-rs.tree/v1",
        "File-by-file directory orientation with top symbols per file",
    ),
    (
        "schema",
        "ripwire-rs.grep/v1",
        "Literal or regex search hits annotated with enclosing AST symbol and context lines",
    ),
    (
        "schema",
        "ripwire-rs.lego/v1",
        "Interface or base type contract and all implementing/extending types",
    ),
    (
        "schema",
        "ripwire-rs.exemplar/v1",
        "Best-in-class existing symbol implementation selected by role and PageRank",
    ),
    (
        "schema",
        "ripwire-rs.recall/v1",
        "Relevant Markdown design docs and notes ranked for a task",
    ),
    (
        "schema",
        "ripwire-rs.from-trace/v1",
        "Stack trace frames mapped onto indexed symbols innermost-first",
    ),
    (
        "schema",
        "ripwire-rs.at/v1",
        "Innermost enclosing symbol chain at FILE:LINE",
    ),
    (
        "schema",
        "ripwire-rs.verify/v1",
        "Formal verification verdict for calls/uses/defines/contains claims",
    ),
    (
        "schema",
        "ripwire-rs.edit-check/v1",
        "1-hop caller arity and visibility compatibility check vs git HEAD",
    ),
    (
        "schema",
        "ripwire-rs.affected/v1",
        "Test files transitively reaching modified files or symbols",
    ),
    (
        "schema",
        "ripwire-rs.test-gate/v1",
        "Pre-PR gate joining tests to run and untested blast radius",
    ),
    (
        "schema",
        "ripwire-rs.situ/v1",
        "Situational awareness diff bundle: blast radius, tests, co-change, hotspots",
    ),
    (
        "schema",
        "ripwire-rs.safe-delete/v1",
        "Deletion safety assessment joining callers, uses, tests, and git history",
    ),
    (
        "schema",
        "ripwire-rs.quality-delta/v1",
        "Code quality regressions vs git HEAD or pinned baseline",
    ),
    (
        "schema",
        "ripwire-rs.quality-panel/v1",
        "Unified 6-family code quality panel (structural, lexical, confusion, historical, colocation, state); state = mutable non-local state only, configuration reads excluded (state_lens=mutable); historical cut = p90 of changed files clamped 3..14",
    ),
    (
        "schema",
        "ripwire-rs.dead-code/v1",
        "Internal functions and methods with zero incoming callers",
    ),
    (
        "schema",
        "ripwire-rs.communities/v1",
        "Leiden modularity call-graph community clusters",
    ),
    (
        "schema",
        "ripwire-rs.deps/v1",
        "File-to-file dependency graph with Lakos CCD/ACD and Martin instability",
    ),
    (
        "schema",
        "ripwire-rs.arch/v1",
        "Architectural layering rule violations across directory/glob boundaries",
    ),
    (
        "schema",
        "ripwire-rs.clones/v1",
        "Type-1, Type-2, and Type-3 AST code clone groups",
    ),
    (
        "schema",
        "ripwire-rs.hotspots/v1",
        "Maintenance risk hotspots (recency-weighted git churn * cognitive complexity)",
    ),
    (
        "schema",
        "ripwire-rs.cochange/v1",
        "Historical git co-change coupling (Fowler's Shotgun Surgery)",
    ),
    (
        "schema",
        "ripwire-rs.cochange-groups/v1",
        "Multi-file greedy co-change clusters",
    ),
    (
        "schema",
        "ripwire-rs.owners/v1",
        "Recency-weighted author ownership and bus-factor per file",
    ),
    (
        "schema",
        "ripwire-rs.whereis/v1",
        "Branch and commit-history existence lookup for a symbol",
    ),
    (
        "schema",
        "ripwire-rs.stray-content/v1",
        "Divergent branch work vs merge-base with HEAD",
    ),
    (
        "schema",
        "ripwire-rs.doc-drift/v1",
        "Out-of-date file:line and backticked symbol anchors in Markdown docs",
    ),
    (
        "schema",
        "ripwire-rs.flags/v1",
        "Compile-time gates, CMake options, and environment variable reads",
    ),
    (
        "schema",
        "ripwire-rs.lint/v1",
        "AST code smell findings across indexed symbols",
    ),
    (
        "schema",
        "ripwire-rs.nonlocal-state/v1",
        "Functions reading or writing nonlocal state: mutable bindings, and constants the indexed files mutate",
    ),
    (
        "schema",
        "ripwire-rs.context-ratio/v1",
        "Local-reasoning lens measuring cross-file callee context ratio",
    ),
    (
        "schema",
        "ripwire-rs.comment-coherence/v1",
        "Doc-comment to identifier lexical coherence metrics",
    ),
    (
        "schema",
        "ripwire-rs.dmm/v1",
        "Delta Maintainability Model scalar in [0,1]",
    ),
    ("tag", "r", "Root map container element"),
    ("tag", "f", "Source file container element"),
    ("tag", "s", "Indexed symbol definition or reference row"),
    (
        "tag",
        "c",
        "Resolved outgoing call edge from enclosing symbol",
    ),
    ("tag", "b", "Grep context line preceding the matched line"),
    ("tag", "a", "Grep context line following the matched line"),
    (
        "tag",
        "hit",
        "Matched grep source line with enclosing symbol attribution",
    ),
    (
        "attr",
        "p",
        "File path relative to workspace root (or file:line locator)",
    ),
    ("attr", "n", "Symbol or identifier name"),
    (
        "attr",
        "t",
        "Symbol kind tag (fn, method, cls, struct, iface, var, sec, macro, field, modscope)",
    ),
    (
        "attr",
        "sc",
        "Enclosing class, struct, trait, module, or namespace scope",
    ),
    ("attr", "l", "1-based source start line number"),
    ("attr", "lang", "Source language tag of the definition"),
    (
        "attr",
        "test",
        "1 when the definition is test or bench code",
    ),
    (
        "attr",
        "sel",
        "Selector spelling that binds this definition alone; pass it to any symbol verb",
    ),
    ("attr", "el", "1-based source end line number"),
    ("attr", "k", "PageRank or HITS centrality score"),
    (
        "attr",
        "in",
        "Incoming caller degree (fan-in) or enclosing symbol name",
    ),
    ("attr", "out", "Outgoing callee degree (fan-out)"),
    ("attr", "cx", "McCabe cyclomatic complexity"),
    ("attr", "ccx", "SonarSource Campbell cognitive complexity"),
    (
        "attr",
        "loc",
        "Lines of code spanned by the symbol definition",
    ),
    ("attr", "params", "Declared formal parameter count"),
    ("attr", "nest", "Maximum control-flow nesting depth"),
    (
        "attr",
        "humps",
        "Separate entries into nesting at or past the bar (deep-nesting regions)",
    ),
    (
        "attr",
        "deep",
        "Lines inside deep-nesting regions, each line counted once",
    ),
    (
        "attr",
        "ev",
        "Essential complexity (irreducible unstructured control flow)",
    ),
    (
        "attr",
        "cbo",
        "Coupling Between Objects (distinct external types/modules referenced)",
    ),
    ("attr", "amp", "Change amplification factor"),
    (
        "attr",
        "amb",
        "Count of outgoing calls resolved ambiguously across multiple candidates",
    ),
    (
        "attr",
        "prov",
        "Edge binding provenance (scip, binding, import, split, final-segment)",
    ),
    (
        "attr",
        "role",
        "Reference role (call, read, write, import, extends, macro, type, value, hub, authority)",
    ),
    ("attr", "dist", "Call-graph hop distance from seed symbol"),
    (
        "attr",
        "shown",
        "Number of items emitted in the current response window",
    ),
    (
        "attr",
        "total",
        "Total number of matching items before window truncation",
    ),
    (
        "attr",
        "capped",
        "1 if output was truncated by top-k or byte/token budget, 0 otherwise",
    ),
    (
        "attr",
        "to",
        "Count of <to> rows: functions whose every indexed caller is test code (--dead-code); outside count=",
    ),
    (
        "attr",
        "has_more",
        "1 if additional pages exist beyond shown items",
    ),
    (
        "attr",
        "next_offset",
        "Zero-based offset cursor to fetch the next page",
    ),
    (
        "attr",
        "offset",
        "Zero-based starting offset of the current page",
    ),
    ("attr", "limit", "Maximum items requested per page"),
    (
        "attr",
        "est_tokens",
        "Estimated LLM token cost of the serialized payload",
    ),
    (
        "attr",
        "pr_iters",
        "Power-iteration steps taken for PageRank convergence",
    ),
    (
        "attr",
        "layer",
        "Architectural layer classification (e.g. test, domain, cli)",
    ),
];

pub fn serialize_legend_dict_xml<W: Write>(out: &mut W) -> io::Result<()> {
    writeln!(
        out,
        "<!-- ripwire-rs legend-dict schema=ripwire-rs.legend-dict/v1: full attribute and schema encyclopedia (dictv={LEGEND_DICT_VERSION}). -->"
    )?;
    writeln!(
        out,
        "<legend-dict schema=\"ripwire-rs.legend-dict/v1\" dictv=\"{LEGEND_DICT_VERSION}\" entries=\"{}\">",
        LEGEND_DICT_ENTRIES.len()
    )?;
    for &(cat, key, desc) in LEGEND_DICT_ENTRIES {
        writeln!(
            out,
            "  <entry cat=\"{cat}\" k=\"{key}\" v=\"{}\"/>",
            desc.xml_escaped()
        )?;
    }
    writeln!(out, "</legend-dict>")
}
