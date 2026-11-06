use super::{
    ComplexityKinds, DefCaptureGateContext, DialectGroup, LanguageSemantics, SyntaxKinds,
    VisibilityMode,
};
use crate::domain::SymKind;

pub struct ElixirLanguage;
pub static ELIXIR_LANG: ElixirLanguage = ElixirLanguage;

pub(super) const BEAM_STD_MEMBERS: &[&str] = &[
    "get",
    "put",
    "delete",
    "fetch",
    "fetch!",
    "merge",
    "keys",
    "values",
    "has_key?",
    "new",
    "to_list",
    "from_struct",
    "update",
    "update!",
    "pop",
    "take",
    "drop",
    "split",
    "map",
    "each",
    "filter",
    "reject",
    "reduce",
    "find",
    "any?",
    "all?",
    "member?",
    "count",
    "empty?",
    "concat",
    "flat_map",
    "join",
    "sort",
    "reverse",
    "uniq",
    "at",
    "first",
    "last",
    "trim",
    "downcase",
    "upcase",
    "replace",
    "starts_with?",
    "ends_with?",
    "contains?",
    "inspect",
    "puts",
    "info",
    "debug",
    "warn",
    "warning",
    "error",
];

pub(super) const BEAM_STD_QUALIFIERS: &[&str] = &[
    "Enum",
    "Map",
    "Keyword",
    "List",
    "String",
    "Integer",
    "Float",
    "Tuple",
    "MapSet",
    "Stream",
    "File",
    "Path",
    "IO",
    "Process",
    "Task",
    "Agent",
    "GenServer",
    "Supervisor",
    "Application",
    "Logger",
    "Kernel",
    "System",
    "DateTime",
    "NaiveDateTime",
    "Date",
    "Time",
    "URI",
    "Regex",
    "Base",
    "Jason",
    "ets",
    "erlang",
    "lists",
    "maps",
    "proplists",
    "binary",
    "io",
    "file",
    "timer",
];

const ELIXIR_FN_KEYWORDS: &[&str] = &[
    "def",
    "defp",
    "defmacro",
    "defmacrop",
    "defguard",
    "defguardp",
    "defdelegate",
];

const ELIXIR_TYPED_ATTRS: &[&str] = &["type", "typep", "opaque", "callback", "macrocallback"];

const ELIXIR_METADATA_ATTRS: &[&str] = &[
    "doc",
    "moduledoc",
    "typedoc",
    "spec",
    "impl",
    "behaviour",
    "behavior",
    "derive",
    "enforce_keys",
    "dialyzer",
    "on_load",
    "before_compile",
    "after_compile",
    "after_verify",
    "optional_callbacks",
    "compile",
    "external_resource",
    "vsn",
    "file",
];

const ELIXIR_SPECIAL_CALLS: &[&str] = &[
    "defmodule",
    "defprotocol",
    "defimpl",
    "defstruct",
    "defexception",
    "defoverridable",
    "alias",
    "import",
    "require",
    "use",
    "quote",
    "unquote",
    "unquote_splicing",
    "case",
    "cond",
    "for",
    "if",
    "unless",
    "with",
    "receive",
    "try",
    "test",
];

fn elixir_arguments<'a>(node: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    (0..node.named_child_count())
        .filter_map(|i| node.named_child(i))
        .find(|c| c.kind() == "arguments")
}

fn non_comment_args(args: tree_sitter::Node<'_>) -> impl Iterator<Item = tree_sitter::Node<'_>> {
    (0..args.named_child_count())
        .filter_map(move |i| args.named_child(i))
        .filter(|c| c.kind() != "comment")
}

fn elixir_first_arg<'a>(node: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    non_comment_args(elixir_arguments(node)?).next()
}

fn unwrap_when_guard<'a>(
    head: tree_sitter::Node<'a>,
    source_bytes: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    let op = head.child_by_field_name("operator")?;
    if op.utf8_text(source_bytes).ok() == Some("when") {
        head.child_by_field_name("left")
    } else {
        None
    }
}

fn count_non_comment_args(args: tree_sitter::Node<'_>) -> u16 {
    non_comment_args(args).count() as u16
}

pub fn elixir_param_count(def_node: tree_sitter::Node<'_>, source_bytes: &[u8]) -> u16 {
    let Some(mut head) = elixir_first_arg(def_node) else {
        return 0;
    };
    if head.kind() == "binary_operator" {
        match unwrap_when_guard(head, source_bytes) {
            Some(left) => head = left,
            None => return 2,
        }
    }
    if head.kind() == "unary_operator" {
        return 1;
    }
    elixir_arguments(head)
        .map(count_non_comment_args)
        .unwrap_or(0)
}

fn is_elixir_call_def_allowed(dn: tree_sitter::Node<'_>, ctx: &DefCaptureGateContext<'_>) -> bool {
    let Some(target) = dn
        .child_by_field_name("target")
        .and_then(|t| t.utf8_text(ctx.source_bytes).ok())
    else {
        return false;
    };
    if ctx.kind == SymKind::Section {
        return matches!(target, "defmodule" | "defprotocol" | "defimpl");
    }
    if target == "test" {
        return ctx.name_node.is_some_and(|n| n.kind() == "string");
    }
    ELIXIR_FN_KEYWORDS.contains(&target) && !matches!(ctx.name, "unquote" | "when")
}

fn is_elixir_attr_def_allowed(dn: tree_sitter::Node<'_>, ctx: &DefCaptureGateContext<'_>) -> bool {
    if ctx.sub == Some("attribute") {
        return !ELIXIR_TYPED_ATTRS.contains(&ctx.name)
            && !ELIXIR_METADATA_ATTRS.contains(&ctx.name);
    }
    dn.child_by_field_name("operand")
        .and_then(|op| op.child_by_field_name("target"))
        .and_then(|t| t.utf8_text(ctx.source_bytes).ok())
        .is_some_and(|t| ELIXIR_TYPED_ATTRS.contains(&t))
}

// `if`/`case`/`cond` are plain `call` nodes in this grammar, so only their clause nodes are visible.
const KINDS: ComplexityKinds = ComplexityKinds {
    paths: &["stab_clause"],
    flat: &["rescue_block", "catch_block"],
    nesting: &[],
    nest_only: &[],
    else_kinds: &["else_block"],
    else_bodies: &[],
    jumps: &[],
    logical_nodes: &[],
    binary_nodes: &["binary_operator"],
    logical_ops: &["&&", "||", "and", "or"],
};

const SYNTAX: SyntaxKinds = SyntaxKinds {
    member_access: &["call", "dot"],
    body_blocks: &["block", "do_block"],
    identifiers: &["identifier"],
    ..SyntaxKinds::NONE
};

impl LanguageSemantics for ElixirLanguage {
    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }

    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }

    fn tag(&self) -> &'static str {
        "ex"
    }

    fn lint_groups(&self) -> &'static [crate::languages::LintGroup] {
        &[crate::languages::LintGroup::Naming]
    }
    super::impl_grammar_method!(crate tree_sitter_elixir::LANGUAGE, "tags.scm");

    fn extensions(&self) -> &'static [&'static str] {
        &["ex", "exs"]
    }

    fn display_color(&self) -> &'static str {
        "#6e4a7e"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Beam
    }

    fn uses_hash_comments(&self) -> bool {
        true
    }

    fn visibility_mode(&self) -> VisibilityMode {
        VisibilityMode::ByNameConvention
    }

    fn resolve_def_params_and_name(
        &self,
        def_node: Option<tree_sitter::Node<'_>>,
        source_bytes: &[u8],
        kind: SymKind,
        orig_name: &str,
    ) -> (crate::gate::contract::ParamArity, String) {
        let params = def_node
            .map(|n| elixir_param_count(n, source_bytes))
            .unwrap_or(0);
        let name = if kind == SymKind::Function {
            format!("{orig_name}/{params}")
        } else {
            orig_name.to_string()
        };
        (crate::gate::contract::ParamArity::fixed(params), name)
    }

    fn format_callee_name(&self, name: &str, arg_count: u16, arg_count_known: bool) -> String {
        let arity = if arg_count_known { arg_count } else { 0 };
        format!("{name}/{arity}")
    }

    fn extract_custom_scope(
        &self,
        parent: tree_sitter::Node<'_>,
        name_node: tree_sitter::Node<'_>,
        source_bytes: &[u8],
    ) -> Option<String> {
        if parent.kind() != "call" {
            return None;
        }
        let target = parent
            .child_by_field_name("target")?
            .utf8_text(source_bytes)
            .ok()?;
        if !matches!(target, "defmodule" | "defprotocol") {
            return None;
        }
        crate::ingest::distinct_scope_tail(elixir_first_arg(parent)?, name_node, source_bytes, ".")
    }

    fn is_symbol_name_public(&self, sym_name: &str) -> bool {
        !sym_name.starts_with('_') && !sym_name.starts_with('#')
    }

    fn is_def_capture_allowed(&self, ctx: &DefCaptureGateContext<'_>) -> bool {
        let Some(dn) = ctx.def_node else {
            return false;
        };
        match dn.kind() {
            "call" => is_elixir_call_def_allowed(dn, ctx),
            "unary_operator" => is_elixir_attr_def_allowed(dn, ctx),
            _ => true,
        }
    }

    fn should_ignore_call(&self, _name_node: Option<tree_sitter::Node<'_>>, name: &str) -> bool {
        ELIXIR_FN_KEYWORDS.contains(&name) || ELIXIR_SPECIAL_CALLS.contains(&name)
    }

    fn is_std_or_builtin_member(&self, name: &str) -> bool {
        let bare = name.split('/').next().unwrap_or(name);
        BEAM_STD_MEMBERS.contains(&bare)
    }

    fn std_qualifier_names(&self) -> &'static [&'static str] {
        BEAM_STD_QUALIFIERS
    }
}
