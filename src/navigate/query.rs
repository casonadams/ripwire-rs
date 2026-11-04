use super::find_symbols_by_name;
use crate::domain::{IngestResult, Interner, NodeId};
use crate::graph::Graph;
use crate::serialize::{PageRequest, Paged, XmlEscapeExt, row_bytes};
use std::collections::BTreeSet;
use std::io::{self, Write};

#[derive(Debug, PartialEq, Eq, Clone)]
enum Token {
    Ident(String),
    Str(String),
    Num(usize),
    LParen,
    RParen,
    Comma,
}

fn read_chars_while(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    pred: impl Fn(char) -> bool,
) -> String {
    let mut s = String::new();
    while let Some(&nc) = chars.peek() {
        if !pred(nc) {
            break;
        }
        s.push(nc);
        chars.next();
    }
    s
}

fn read_quoted_string(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, quote: char) -> String {
    let s = read_chars_while(chars, |c| c != quote);
    chars.next();
    s
}

fn read_digits(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<usize> {
    read_chars_while(chars, |c| c.is_ascii_digit())
        .parse::<usize>()
        .ok()
}

fn read_ident(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    read_chars_while(chars, |c| {
        c.is_alphanumeric() || matches!(c, '_' | ':' | '-' | '.')
    })
}

fn next_token(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<Token> {
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        match c {
            '(' => {
                chars.next();
                return Some(Token::LParen);
            }
            ')' => {
                chars.next();
                return Some(Token::RParen);
            }
            ',' => {
                chars.next();
                return Some(Token::Comma);
            }
            '"' | '\'' => {
                chars.next();
                return Some(Token::Str(read_quoted_string(chars, c)));
            }
            '0'..='9' => return read_digits(chars).map(Token::Num),
            _ => {
                let id = read_ident(chars);
                if !id.is_empty() {
                    return Some(Token::Ident(id));
                }
                chars.next();
            }
        }
    }
    None
}

fn tokenize_graph_query(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(tok) = next_token(&mut chars) {
        tokens.push(tok);
    }
    tokens
}

/// Predicate trait for filtering symbol sets in the `--graph-query` functional DSL.
pub trait GraphFilterPredicate: std::fmt::Debug + Send + Sync {
    fn matches(
        &self,
        sym: &crate::domain::Symbol,
        ingest: &IngestResult,
        graph: &Graph,
        interner: &Interner,
    ) -> bool;
}

#[derive(Debug, Clone)]
enum FilterSpec {
    Kind(String),
    File(String),
    Layer(String),
    Fanin(usize),
    Cx(u32),
}

impl GraphFilterPredicate for FilterSpec {
    fn matches(
        &self,
        sym: &crate::domain::Symbol,
        ingest: &IngestResult,
        graph: &Graph,
        interner: &Interner,
    ) -> bool {
        use crate::serialize::PathLayerExt;
        match self {
            Self::Kind(tag) => kind_matches_tag(sym.kind, tag),
            Self::File(pat) => ingest
                .files
                .get(sym.file_id as usize)
                .is_some_and(|f| interner.resolve(&f.path).contains(pat)),
            Self::Layer(layer) => ingest
                .files
                .get(sym.file_id as usize)
                .and_then(|f| interner.resolve(&f.path).builtin_layer())
                .is_some_and(|l| l.eq_ignore_ascii_case(layer)),
            Self::Fanin(min_in) => graph.in_degree(sym.id) >= *min_in,
            Self::Cx(max_cx) => sym.ccx <= *max_cx || sym.cx <= *max_cx,
        }
    }
}

#[derive(Debug, Clone)]
enum GraphExpr {
    All,
    Name(String),
    Callers(Box<GraphExpr>, usize),
    Callees(Box<GraphExpr>, usize),
    Filter(Box<GraphExpr>, std::sync::Arc<dyn GraphFilterPredicate>),
    And(Vec<GraphExpr>),
    Or(Vec<GraphExpr>),
    Not(Box<GraphExpr>),
}

struct GraphQueryParser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> GraphQueryParser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.pos);
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn match_ident(&mut self) -> Option<String> {
        match self.peek() {
            Some(Token::Ident(s)) | Some(Token::Str(s)) => {
                let s = s.clone();
                self.pos += 1;
                Some(s)
            }
            _ => None,
        }
    }

    fn parse_args(&mut self) -> Vec<GraphExpr> {
        let mut args = Vec::new();
        while let Some(expr) = self.parse_expr() {
            args.push(expr);
            if self.peek() == Some(&Token::Comma) {
                self.pos += 1;
            } else {
                break;
            }
        }
        args
    }

    fn parse_expr(&mut self) -> Option<GraphExpr> {
        let tok = self.next()?.clone();
        match &tok {
            Token::Ident(id) => self.parse_ident_expr(id),
            Token::Str(s) => Some(GraphExpr::Name(s.clone())),
            _ => None,
        }
    }

    fn parse_ident_expr(&mut self, id: &str) -> Option<GraphExpr> {
        match id {
            "all" => {
                if self.peek() == Some(&Token::LParen) {
                    self.pos += 1;
                    if self.peek() == Some(&Token::RParen) {
                        self.pos += 1;
                    }
                }
                Some(GraphExpr::All)
            }
            "name" => self.parse_paren_call("name", |p| {
                Some(GraphExpr::Name(p.match_ident().unwrap_or_default()))
            }),
            "callers" => self.parse_closure_expr("callers", true),
            "callees" => self.parse_closure_expr("callees", false),
            "kind" => self.parse_filter_with_opt(
                "kind",
                String::new(),
                Self::match_ident,
                FilterSpec::Kind,
            ),
            "file" => self.parse_filter_with_opt(
                "file",
                String::new(),
                Self::match_ident,
                FilterSpec::File,
            ),
            "layer" => self.parse_filter_with_opt(
                "layer",
                String::new(),
                Self::match_ident,
                FilterSpec::Layer,
            ),
            "fanin" => {
                self.parse_filter_with_opt("fanin", 1, Self::next_num_opt, FilterSpec::Fanin)
            }
            "cx" => self
                .parse_filter_with_opt("cx", 15, Self::next_num_opt, |n| FilterSpec::Cx(n as u32)),
            "and" => self.parse_paren_call("and", |p| Some(GraphExpr::And(p.parse_args()))),
            "or" => self.parse_paren_call("or", |p| Some(GraphExpr::Or(p.parse_args()))),
            "not" => {
                self.parse_paren_call("not", |p| Some(GraphExpr::Not(Box::new(p.parse_expr()?))))
            }
            _ => Some(GraphExpr::Name(id.to_string())),
        }
    }

    fn parse_paren_call(
        &mut self,
        op_name: &str,
        inner: impl FnOnce(&mut Self) -> Option<GraphExpr>,
    ) -> Option<GraphExpr> {
        if self.peek() != Some(&Token::LParen) {
            return Some(GraphExpr::Name(op_name.to_string()));
        }
        self.pos += 1;
        let res = inner(self)?;
        if self.peek() == Some(&Token::RParen) {
            self.pos += 1;
        }
        Some(res)
    }

    fn parse_unary_with_opt<T>(
        &mut self,
        op_name: &str,
        default: T,
        parse_arg: fn(&mut Self) -> Option<T>,
        build: impl FnOnce(Box<GraphExpr>, T) -> GraphExpr,
    ) -> Option<GraphExpr> {
        self.parse_paren_call(op_name, |p| {
            let sub = p.parse_expr()?;
            let arg = if p.peek() == Some(&Token::Comma) {
                p.pos += 1;
                parse_arg(p).unwrap_or(default)
            } else {
                default
            };
            Some(build(Box::new(sub), arg))
        })
    }

    fn next_num_opt(&mut self) -> Option<usize> {
        match self.next() {
            Some(Token::Num(n)) => Some(*n),
            _ => None,
        }
    }

    fn parse_closure_expr(&mut self, op_name: &str, is_callers: bool) -> Option<GraphExpr> {
        self.parse_unary_with_opt(op_name, 1, Self::next_num_opt, |sub, depth| {
            if is_callers {
                GraphExpr::Callers(sub, depth)
            } else {
                GraphExpr::Callees(sub, depth)
            }
        })
    }

    fn parse_filter_with_opt<T>(
        &mut self,
        op_name: &str,
        default: T,
        parse_arg: fn(&mut Self) -> Option<T>,
        build: fn(T) -> FilterSpec,
    ) -> Option<GraphExpr> {
        self.parse_unary_with_opt(op_name, default, parse_arg, |sub, arg| {
            GraphExpr::Filter(sub, std::sync::Arc::new(build(arg)))
        })
    }
}

fn kind_matches_tag(kind: crate::domain::SymKind, tag: &str) -> bool {
    kind.tag() == tag
        || match tag {
            "fn" | "function" => kind == crate::domain::SymKind::Function,
            "method" => kind == crate::domain::SymKind::Method,
            "class" | "cls" => kind == crate::domain::SymKind::Class,
            "struct" => kind == crate::domain::SymKind::Struct,
            "interface" | "iface" => kind == crate::domain::SymKind::Interface,
            "section" | "sec" => kind == crate::domain::SymKind::Section,
            _ => false,
        }
}

fn eval_1hop_closure(
    mut current: BTreeSet<NodeId>,
    depth: usize,
    graph: &Graph,
    is_callers: bool,
) -> BTreeSet<NodeId> {
    for _ in 0..depth {
        let mut next = BTreeSet::new();
        for &id in &current {
            if is_callers {
                next.extend(graph.callers(id).map(|(n, _)| n));
            } else {
                next.extend(graph.callees(id).map(|(n, _)| n));
            }
        }
        current = next;
    }
    current
}

fn eval_filter_predicate(
    base: BTreeSet<NodeId>,
    pred: &dyn GraphFilterPredicate,
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
) -> BTreeSet<NodeId> {
    base.into_iter()
        .filter(|&id| {
            ingest
                .symbols
                .get(id as usize)
                .is_some_and(|sym| pred.matches(sym, ingest, graph, interner))
        })
        .collect()
}

fn eval_and_composite(
    subs: &[GraphExpr],
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
) -> BTreeSet<NodeId> {
    let mut iter = subs.iter();
    let Some(first) = iter.next() else {
        return BTreeSet::new();
    };
    let mut acc = evaluate_graph_expr(first, ingest, graph, interner);
    for sub in iter {
        let next = evaluate_graph_expr(sub, ingest, graph, interner);
        acc = acc.intersection(&next).copied().collect();
    }
    acc
}

fn eval_or_composite(
    subs: &[GraphExpr],
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
) -> BTreeSet<NodeId> {
    let mut acc = BTreeSet::new();
    for sub in subs {
        acc.extend(evaluate_graph_expr(sub, ingest, graph, interner));
    }
    acc
}

fn evaluate_graph_expr(
    expr: &GraphExpr,
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
) -> BTreeSet<NodeId> {
    match expr {
        GraphExpr::All => ingest.symbols.iter().map(|s| s.id).collect(),
        GraphExpr::Name(name) => find_symbols_by_name(name, ingest, interner)
            .into_iter()
            .collect(),
        GraphExpr::Callers(sub, depth) | GraphExpr::Callees(sub, depth) => {
            let base = evaluate_graph_expr(sub, ingest, graph, interner);
            eval_1hop_closure(base, *depth, graph, matches!(expr, GraphExpr::Callers(..)))
        }
        GraphExpr::Filter(sub, pred) => {
            let base = evaluate_graph_expr(sub, ingest, graph, interner);
            eval_filter_predicate(base, pred.as_ref(), ingest, graph, interner)
        }
        GraphExpr::And(subs) => eval_and_composite(subs, ingest, graph, interner),
        GraphExpr::Or(subs) => eval_or_composite(subs, ingest, graph, interner),
        GraphExpr::Not(sub) => {
            let excluded = evaluate_graph_expr(sub, ingest, graph, interner);
            ingest
                .symbols
                .iter()
                .map(|s| s.id)
                .filter(|id| !excluded.contains(id))
                .collect()
        }
    }
}

pub struct GraphQueryReport<'a> {
    pub expr: &'a str,
    pub matched_nodes: &'a [NodeId],
    pub ingest: &'a IngestResult,
    pub interner: &'a Interner,
    pub pr_iters: u32,
    pub gauges: crate::serialize::GraphGauges,
    pub page: PageRequest,
}

impl GraphQueryReport<'_> {
    fn paged_nodes(&self) -> Paged<'_, NodeId> {
        Paged::new(self.matched_nodes, self.page, |&id| {
            row_bytes(|mut w| self.write_node(&mut w, id))
        })
    }

    fn write_node<W: Write>(&self, out: &mut W, node_id: NodeId) -> io::Result<()> {
        if let Some(sym) = self.ingest.sym(node_id) {
            let n = self.interner[sym.name].xml_escaped();
            let p = self.ingest.sym_file_path(sym, self.interner).xml_escaped();
            writeln!(
                out,
                "<s t=\"{}\" n=\"{n}\" p=\"{p}:{}\"/>",
                sym.kind, sym.line
            )?;
        }
        Ok(())
    }
}

impl crate::serialize::XmlReport for GraphQueryReport<'_> {
    fn schema_tag(&self) -> &'static str {
        "query"
    }
    fn schema_version(&self) -> &'static str {
        "ripwire-rs.graph-query/v1"
    }
    fn legend_comment(&self) -> Option<&'static str> {
        Some(
            "ripwire-rs graph-query schema=ripwire-rs.graph-query/v1: graph-query expression over the symbol graph.",
        )
    }
    fn pr_iters(&self) -> Option<u32> {
        Some(self.pr_iters)
    }
    fn graph_gauges(&self) -> Option<crate::serialize::GraphGauges> {
        Some(self.gauges)
    }
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let count = self.matched_nodes.len();
        write!(
            out,
            " expr=\"{}\" count=\"{count}\"{}",
            self.expr.xml_escaped(),
            self.paged_nodes().attrs()
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        for &node_id in self.paged_nodes().rows() {
            self.write_node(out, node_id)?;
        }
        Ok(())
    }
}

pub fn execute_graph_query_expr_paged<W: Write>(
    expr: &str,
    root: &str,
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    page: crate::serialize::PageRequest,
    out: &mut W,
) -> io::Result<()> {
    let tokens = tokenize_graph_query(expr);
    let mut parser = GraphQueryParser {
        tokens: &tokens,
        pos: 0,
    };
    let parsed = parser
        .parse_expr()
        .unwrap_or_else(|| GraphExpr::Name(expr.to_string()));
    let matched_nodes: Vec<NodeId> = evaluate_graph_expr(&parsed, ingest, graph, interner)
        .into_iter()
        .collect();
    let pr_iters = graph
        .page_rank(&crate::rank::PageRankConfig::default())
        .iterations
        .max(1);
    use crate::serialize::XmlReport;
    GraphQueryReport {
        expr,
        matched_nodes: &matched_nodes,
        ingest,
        interner,
        pr_iters,
        gauges: graph.gauges(),
        page,
    }
    .write_xml_report(root, out)
}
