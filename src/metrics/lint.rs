use super::lint_naming;
pub use super::lint_report::{serialize_lint_sarif, serialize_lint_xml};
use super::lint_rules::{FileLintContext, Sink, large_function, symbols_in_file};
use crate::domain::{IngestResult, Interner, Lang};
use crate::languages::LintGroup;
use crate::languages::c::lint::{
    atoms as lint_atoms, cache as lint_cache, structure as lint_structure,
};
use crate::languages::lint::{flow as lint_flow, text as lint_text};
use std::io::{self, Write};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintFinding {
    pub rule: &'static str,
    pub path: String,
    pub line: u32,
    pub pos: u32,
    pub enclosing: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LintRuleDesc {
    pub name: &'static str,
    pub severity: &'static str,
    pub category: &'static str,
    pub group: LintGroup,
    pub since: &'static str,
    pub description: &'static str,
}

pub type CheckFn = fn(&FileLintContext<'_>, &mut Sink<'_>);

#[derive(Debug)]
pub struct LintRule {
    pub desc: LintRuleDesc,
    pub check: CheckFn,
}

macro_rules! rule {
    ($name:literal, $sev:literal, $cat:literal, $group:expr, $since:literal, $desc:literal, $check:path) => {
        LintRule {
            desc: LintRuleDesc {
                name: $name,
                severity: $sev,
                category: $cat,
                group: $group,
                since: $since,
                description: $desc,
            },
            check: $check,
        }
    };
}

pub static LINT_RULES: &[LintRule] = &[
    rule!(
        "c-style-cast",
        "warn",
        "style",
        LintGroup::Cast,
        "v0.1.0",
        "a C-style cast — cppcoreguidelines-pro-type-cstyle-cast prefers the explicit static_cast/const_cast/reinterpret_cast",
        lint_text::c_style_cast
    ),
    rule!(
        "goto",
        "warn",
        "control-flow",
        LintGroup::Goto,
        "v0.1.0",
        "a goto statement — cppcoreguidelines-avoid-goto",
        lint_flow::goto
    ),
    rule!(
        "do-while",
        "info",
        "style",
        LintGroup::DoWhile,
        "v0.1.0",
        "a do/while loop shape (on Swift, its do/catch block shares the same grammar node and also matches)",
        lint_flow::do_while
    ),
    rule!(
        "unsafe-c-fn",
        "error",
        "security",
        LintGroup::Insecure,
        "v0.1.0",
        "a call to an unbounded C string function (strcpy/strcat/sprintf/gets)",
        lint_text::unsafe_c_fn
    ),
    rule!(
        "weak-crypto",
        "error",
        "security",
        LintGroup::Insecure,
        "v0.1.0",
        "a call to a broken hash or cipher (MD5/SHA1/MD4/RC4)",
        lint_text::weak_crypto
    ),
    rule!(
        "redundant-parens",
        "info",
        "style",
        LintGroup::Parens,
        "v0.1.0",
        "a doubly-parenthesized expression — readability-redundant-parentheses",
        lint_text::redundant_parens
    ),
    rule!(
        "suspicious-semicolon",
        "warn",
        "correctness",
        LintGroup::CFamily,
        "v0.1.0",
        "an if-body that is just `;` — bugprone-suspicious-semicolon",
        lint_structure::suspicious_semicolon
    ),
    rule!(
        "typedef-over-using",
        "info",
        "style",
        LintGroup::CFamily,
        "v0.1.0",
        "a C-style typedef struct/union where `using` is preferred",
        lint_structure::typedef_over_using
    ),
    rule!(
        "magic-number",
        "info",
        "maintainability",
        LintGroup::CFamily,
        "v0.1.0",
        "a non-trivial numeric literal inside a function body, outside a const/constexpr init",
        lint_atoms::magic_number
    ),
    rule!(
        "empty-catch",
        "warn",
        "error-masking",
        LintGroup::CFamily,
        "v0.1.0",
        "a catch block with an empty body",
        lint_structure::empty_catch
    ),
    rule!(
        "self-assign",
        "warn",
        "correctness",
        LintGroup::SelfAssign,
        "v0.1.0",
        "x = x — almost always a copy-paste bug",
        lint_text::self_assign
    ),
    rule!(
        "large-function",
        "info",
        "maintainability",
        LintGroup::NativeOnly,
        "v0.1.0",
        "a function body over 80 lines",
        large_function
    ),
    rule!(
        "deep-nesting",
        "info",
        "maintainability",
        LintGroup::NativeOnly,
        "v0.1.0",
        "brace nesting depth over 4 inside a function body",
        lint_structure::deep_nesting
    ),
    rule!(
        "inconsistent-return",
        "info",
        "maintainability",
        LintGroup::NativeOnly,
        "v0.1.0",
        "a bare `return;` mixed with a value-returning return in the same function",
        lint_structure::inconsistent_return
    ),
    rule!(
        "unreachable-code",
        "warn",
        "correctness",
        LintGroup::Unreachable,
        "v0.1.0",
        "a statement after an unconditional return/break/continue/throw/raise in the same block",
        lint_flow::unreachable_code
    ),
    rule!(
        "naming-short",
        "info",
        "naming",
        LintGroup::Naming,
        "v0.2.2",
        "a 1-2 letter Function/Method/Var name — visible far beyond any tiny scope [Beniamini/Hofmeister]",
        lint_naming::short
    ),
    rule!(
        "naming-wordy",
        "info",
        "naming",
        LintGroup::Naming,
        "v0.2.2",
        "more than 5 split tokens in one name [Butler; AlSuhaibani]",
        lint_naming::wordy
    ),
    rule!(
        "naming-series",
        "info",
        "naming",
        LintGroup::Naming,
        "v0.2.2",
        "foo1/foo2/... digit-suffix siblings sharing a base name in one scope [Butler]",
        lint_naming::series
    ),
    rule!(
        "naming-underscore",
        "info",
        "naming",
        LintGroup::Naming,
        "v0.2.2",
        "internal consecutive underscores, or a C-family reserved __x/_X form [Butler]",
        lint_naming::underscore
    ),
    rule!(
        "naming-case",
        "info",
        "naming",
        LintGroup::Naming,
        "v0.2.2",
        "snake_case and camelCase mixed inside one name [Butler]",
        lint_naming::mixed_case
    ),
    rule!(
        "naming-predicate",
        "info",
        "naming",
        LintGroup::Naming,
        "v0.2.2",
        "an is/has/can/should/was-prefixed name whose KNOWN return type is not bool-like [LAPD A2]",
        lint_naming::predicate
    ),
    rule!(
        "naming-setter",
        "info",
        "naming",
        LintGroup::Naming,
        "v0.2.2",
        "a set-prefixed name whose KNOWN return type is not void-like [LAPD A3]",
        lint_naming::setter
    ),
    rule!(
        "naming-confusable",
        "info",
        "naming",
        LintGroup::Naming,
        "v0.2.2",
        "a co-visible pair within edit distance <=2, reordered tokens, or a bare/digit-suffixed twin [Namesake]",
        lint_naming::confusable
    ),
    rule!(
        "naming-uninformative",
        "info",
        "naming",
        LintGroup::Naming,
        "v0.2.2",
        "every split subtoken is corpus-ubiquitous (BM25 idf) on a body past the size floor — fires only at the low end [Sparck Jones 1972]",
        lint_naming::uninformative
    ),
    rule!(
        "atom-comma-operator",
        "info",
        "readability",
        LintGroup::CFamily,
        "v0.2.2",
        "the comma operator inside an expression (never a for-header comma) [Gopstein FSE 2017]",
        lint_atoms::comma_operator
    ),
    rule!(
        "atom-embedded-crement",
        "info",
        "readability",
        LintGroup::CFamily,
        "v0.2.2",
        "++/-- evaluated inside a larger expression (never a whole statement) [Gopstein FSE 2017]",
        lint_atoms::embedded_crement
    ),
    rule!(
        "atom-assign-as-value",
        "info",
        "readability",
        LintGroup::CFamily,
        "v0.2.2",
        "an assignment whose VALUE is consumed (a condition, an argument, ...) [Gopstein FSE 2017]",
        lint_atoms::assign_as_value
    ),
    rule!(
        "atom-nested-ternary",
        "info",
        "readability",
        LintGroup::CFamily,
        "v0.2.2",
        "a conditional expression inside a conditional expression [Gopstein FSE 2017]",
        lint_atoms::nested_ternary
    ),
    rule!(
        "atom-implicit-predicate",
        "info",
        "readability",
        LintGroup::CFamily,
        "v0.2.2",
        "arithmetic (or a non-0/1 integer literal) used where a truth value is expected [Gopstein FSE 2017]",
        lint_atoms::implicit_predicate
    ),
    rule!(
        "atom-octal-literal",
        "info",
        "readability",
        LintGroup::CFamily,
        "v0.2.2",
        "a leading-zero integer literal — 0755 is 493, not 755 [Gopstein FSE 2017]",
        lint_atoms::octal_literal
    ),
    rule!(
        "atom-reversed-subscript",
        "info",
        "readability",
        LintGroup::CFamily,
        "v0.2.2",
        "1[arr] — legal C, almost never intended [Gopstein FSE 2017]",
        lint_atoms::reversed_subscript
    ),
    rule!(
        "cache-node-container",
        "info",
        "performance",
        LintGroup::CFamily,
        "v0.2.2",
        "a node-based std container (map/list/set/...) — one heap node per element, a dependent pointer chase per traversal step",
        lint_cache::node_container
    ),
    rule!(
        "cache-vector-of-raw-ptr",
        "info",
        "performance",
        LintGroup::CFamily,
        "v0.2.2",
        "vector<T*> — contiguous handles, scattered payloads",
        lint_cache::vector_of_raw_ptr
    ),
    rule!(
        "cache-vector-of-indirect",
        "info",
        "performance",
        LintGroup::CFamily,
        "v0.2.2",
        "vector<unique_ptr/shared_ptr/vector<...>> — an indirection per element",
        lint_cache::vector_of_indirect
    ),
    rule!(
        "cache-heap-alloc-in-loop",
        "info",
        "performance",
        LintGroup::CFamily,
        "v0.2.2",
        "new/malloc/calloc/realloc/strdup inside a loop body — per-iteration allocation scatters the loop's output across the heap",
        lint_cache::heap_alloc_in_loop
    ),
    rule!(
        "cache-pointer-chase-loop",
        "info",
        "performance",
        LintGroup::CFamily,
        "v0.2.2",
        "p = p->next inside a loop — a serial dependent-load chain the prefetcher cannot predict",
        lint_cache::pointer_chase_loop
    ),
    rule!(
        "cache-gather-subscript",
        "info",
        "performance",
        LintGroup::CFamily,
        "v0.2.2",
        "a[b[i]] inside a loop — gather/scatter, random access by construction",
        lint_cache::gather_subscript
    ),
    rule!(
        "cache-shared-ptr-by-value",
        "info",
        "performance",
        LintGroup::CFamily,
        "v0.2.2",
        "a by-value shared_ptr parameter — atomic refcount traffic per call",
        lint_cache::shared_ptr_by_value
    ),
    rule!(
        "cache-manual-prefetch",
        "info",
        "performance",
        LintGroup::CFamily,
        "v0.2.2",
        "an existing _mm_prefetch/__builtin_prefetch call, flagged for re-measurement (2007-era wins are often a wash today)",
        lint_cache::manual_prefetch
    ),
];

pub const DEFAULT_RULE_BUDGET: usize = 5000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintOptions {
    pub select: Vec<String>,
    pub ignore: Vec<String>,
    pub budget: usize,
}

impl Default for LintOptions {
    fn default() -> Self {
        Self {
            select: Vec::new(),
            ignore: Vec::new(),
            budget: DEFAULT_RULE_BUDGET,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RuleStat {
    pub rule: &'static LintRule,
    pub count: usize,
    pub capped: bool,
    pub applicable: bool,
}

#[derive(Debug)]
pub struct LintScan {
    pub findings: Vec<LintFinding>,
    pub stats: Vec<RuleStat>,
    pub options: LintOptions,
}

#[must_use]
pub fn rule_applies(rule: &LintRuleDesc, lang: Lang) -> bool {
    let sem = lang.semantics();
    sem.lint_groups().contains(&rule.group) || sem.lint_extra_rules().contains(&rule.name)
}

fn matching_rules(prefixes: &[String], flag: &str) -> Result<Vec<&'static str>, String> {
    let mut names = Vec::new();
    for p in prefixes {
        let before = names.len();
        names.extend(
            LINT_RULES
                .iter()
                .map(|r| r.desc.name)
                .filter(|n| n.starts_with(p.as_str())),
        );
        if names.len() == before {
            return Err(format!(
                "{flag}: '{p}' matches no rule or family — see --lint-catalog for the full registry"
            ));
        }
    }
    Ok(names)
}

pub fn selected_rules(options: &LintOptions) -> Result<Vec<&'static LintRule>, String> {
    let selected = matching_rules(&options.select, "--lint-select")?;
    let ignored = matching_rules(&options.ignore, "--lint-ignore")?;
    Ok(LINT_RULES
        .iter()
        .filter(|r| {
            (selected.is_empty() || selected.contains(&r.desc.name))
                && !ignored.contains(&r.desc.name)
        })
        .collect())
}

fn scan_file(ctx: &FileLintContext<'_>, rules: &[&'static LintRule], buckets: &mut [RuleBucket]) {
    for (rule, bucket) in rules.iter().zip(buckets.iter_mut()) {
        if rule_applies(&rule.desc, ctx.lang) {
            let mut sink = Sink::new(&rule.desc, ctx);
            (rule.check)(ctx, &mut sink);
            bucket.capped |= sink.capped;
            bucket.findings.append(&mut sink.out);
        }
    }
}

#[derive(Default)]
struct RuleBucket {
    findings: Vec<LintFinding>,
    capped: bool,
}

fn finish_bucket(
    rule: &'static LintRule,
    mut bucket: RuleBucket,
    budget: usize,
) -> (RuleStat, Vec<LintFinding>) {
    bucket
        .findings
        .sort_by(|a, b| (&a.path, a.pos).cmp(&(&b.path, b.pos)));
    let capped = bucket.capped || bucket.findings.len() >= budget;
    bucket.findings.truncate(budget);
    let stat = RuleStat {
        rule,
        count: bucket.findings.len(),
        capped,
        applicable: false,
    };
    (stat, bucket.findings)
}

pub fn run_lint_scan(
    ingest: &IngestResult,
    interner: &Interner,
    read: &dyn Fn(&str) -> Option<String>,
    options: LintOptions,
) -> Result<LintScan, String> {
    let rules = selected_rules(&options)?;
    let corpus = lint_naming::NameCorpus::build(ingest, interner);
    let mut buckets: Vec<RuleBucket> = rules.iter().map(|_| RuleBucket::default()).collect();
    let mut file_langs = std::collections::BTreeSet::new();
    for fi in &ingest.files {
        if fi.lang.semantics().is_prose_document() {
            continue;
        }
        file_langs.insert(fi.lang as usize);
        let path = interner.resolve(&fi.path);
        let Some(content) = read(path) else { continue };
        let syms = symbols_in_file(ingest, interner, fi.id);
        let ctx = FileLintContext::new(path, fi.lang, &content, syms, &corpus);
        scan_file(&ctx, &rules, &mut buckets);
    }
    let mut findings = Vec::new();
    let mut stats = Vec::new();
    for (rule, bucket) in rules.iter().zip(buckets) {
        let (mut stat, mut rows) = finish_bucket(rule, bucket, options.budget);
        stat.applicable = crate::languages::ALL_LANGS
            .iter()
            .any(|&l| file_langs.contains(&(l as usize)) && rule_applies(&rule.desc, l));
        stats.push(stat);
        findings.append(&mut rows);
    }
    findings.sort_by(|a, b| {
        (&a.path, a.line, a.pos, a.rule, &a.message)
            .cmp(&(&b.path, b.line, b.pos, b.rule, &b.message))
    });
    Ok(LintScan {
        findings,
        stats,
        options,
    })
}

pub fn run_lint_scan_in_root(
    root: &str,
    ingest: &IngestResult,
    interner: &Interner,
    options: LintOptions,
) -> Result<LintScan, String> {
    let root_path = std::path::Path::new(root);
    let read = |p: &str| crate::crawler::read_rel_string(root_path, p).ok();
    run_lint_scan(ingest, interner, &read, options)
}

pub fn serialize_lint_catalog<W: Write>(out: &mut W) -> io::Result<()> {
    writeln!(
        out,
        "<!-- ripwire-rs lint-catalog schema=ripwire-rs.lint-catalog/v1: the built-in lint rule registry: <rule name= sev= cat= lang= since=>. rules=N: rules in the built-in registry, one rule row each, never cut. -->\n<lintcatalog schema=\"ripwire-rs.lint-catalog/v1\" rules=\"{}\">",
        LINT_RULES.len()
    )?;
    for r in LINT_RULES.iter().map(|r| &r.desc) {
        let langs = crate::languages::ALL_LANGS
            .iter()
            .filter(|&&lang| rule_applies(r, lang))
            .map(|lang| lang.tag())
            .collect::<Vec<_>>()
            .join(",");
        writeln!(
            out,
            "  <rule name=\"{}\" sev=\"{}\" cat=\"{}\" lang=\"{}\" since=\"{}\">{}</rule>",
            r.name,
            r.severity,
            r.category,
            langs,
            r.since,
            crate::serialize::XmlEscaped(r.description)
        )?;
    }
    writeln!(out, "</lintcatalog>")
}
