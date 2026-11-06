use super::{
    ComplexityKinds, DefCaptureGateContext, DialectGroup, LanguageSemantics, SyntaxKinds,
    VisibilityMode,
};
use crate::domain::Lang;

mod types;

pub struct RubyLanguage;
pub static RUBY_LANG: RubyLanguage = RubyLanguage;

const RUBY_STD_MEMBERS: &[&str] = &[
    "new",
    "each",
    "map",
    "select",
    "reject",
    "find",
    "detect",
    "collect",
    "inject",
    "reduce",
    "flat_map",
    "compact",
    "flatten",
    "uniq",
    "sort",
    "sort_by",
    "reverse",
    "first",
    "last",
    "take",
    "drop",
    "count",
    "length",
    "size",
    "empty?",
    "include?",
    "any?",
    "all?",
    "none?",
    "nil?",
    "to_s",
    "to_i",
    "to_f",
    "to_a",
    "to_h",
    "to_sym",
    "to_json",
    "inspect",
    "freeze",
    "dup",
    "clone",
    "keys",
    "values",
    "fetch",
    "delete",
    "merge",
    "slice",
    "strip",
    "chomp",
    "gsub",
    "sub",
    "split",
    "join",
    "downcase",
    "upcase",
    "start_with?",
    "end_with?",
    "match",
    "match?",
];

const RUBY_AMBIENT_CORE: &[&str] = &[
    "write", "respond_to?", "include", "extend", "bytesize", "split", "puts", "print", "p", "pp", "printf",
    "sprintf", "format", "gets", "readline", "readlines", "close", "closed?", "flush", "rewind", "eof?",
    "string", "sync", "fail", "raise", "warn", "exit", "send", "public_send", "freeze", "dup", "clone",
    "to_s", "to_str", "to_proc", "inspect", "is_a?", "kind_of?", "instance_of?", "tap", "then", "each_line",
];

const RUBY_AMBIENT_SHARED: &[&str] = &["call", "each", "read", "select"];

const RUBY_STD_QUALIFIERS: &[&str] = &[
    "File", "Dir", "IO", "JSON", "YAML", "Time", "Date", "DateTime", "Regexp", "Kernel", "Object",
    "Array", "Hash", "String", "Integer", "Float", "Symbol", "Proc", "Thread", "Mutex", "ENV",
    "Math", "URI",
];

const KINDS: ComplexityKinds = ComplexityKinds {
    paths: &["when"],
    flat: &["elsif"],
    nesting: &[
        "if",
        "unless",
        "while",
        "until",
        "for",
        "rescue",
        "if_modifier",
        "unless_modifier",
        "while_modifier",
        "until_modifier",
        "rescue_modifier",
        "conditional",
    ],
    nest_only: &["case"],
    else_kinds: &["else"],
    else_bodies: &[],
    jumps: &["break", "next", "return"],
    logical_nodes: &[],
    binary_nodes: &["binary"],
    logical_ops: &["&&", "||", "and", "or"],
};

const SYNTAX: SyntaxKinds = SyntaxKinds {
    scope_containers: &["class", "module"],
    member_access: &["call"],
    body_blocks: &["block", "do_block", "body_statement"],
    identifiers: &["identifier"],
    binding_patterns: &["array_pattern"],
    ..SyntaxKinds::NONE
};

impl LanguageSemantics for RubyLanguage {
    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }

    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }

    fn tag(&self) -> &'static str {
        "rb"
    }

    fn read_bindings<'a>(
        &self,
        root: tree_sitter::Node<'_>,
        source: &'a [u8],
    ) -> super::reads::ReadBindings<'a> {
        types::typed_read_bindings(root, source)
    }

    fn implementations(
        &self,
        root: tree_sitter::Node<'_>,
        source: &[u8],
    ) -> Vec<super::Implementation> {
        types::class_implementations(root, source)
    }

    super::impl_class_dispatch_flags!();

    fn constructed_receiver<'a>(
        &self,
        receiver: tree_sitter::Node<'_>,
        source: &'a [u8],
    ) -> Option<&'a str> {
        types::constructed_type(receiver, source)
    }

    fn receiver_text<'a>(&self, text: &'a str) -> std::borrow::Cow<'a, str> {
        if let Some(ivar) = text.strip_prefix('@').filter(|v| !v.starts_with('@')) {
            return std::borrow::Cow::Owned(format!("self.{ivar}"));
        }
        let leaf = text.rsplit("::").next().unwrap_or(text);
        let constant_path = text.contains("::")
            && text
                .split("::")
                .all(|seg| seg.starts_with(|c: char| c.is_ascii_uppercase()));
        std::borrow::Cow::Borrowed(if constant_path { leaf } else { text })
    }

    fn lint_groups(&self) -> &'static [crate::languages::LintGroup] {
        &[crate::languages::LintGroup::Naming]
    }

    fn display_color(&self) -> &'static str {
        "#701516"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Isolated(Lang::Ruby)
    }

    fn parameters_shadow_calls(&self) -> bool {
        false
    }

    fn uses_hash_comments(&self) -> bool {
        true
    }

    fn visibility_mode(&self) -> VisibilityMode {
        VisibilityMode::ByNameConvention
    }

    fn is_symbol_name_public(&self, sym_name: &str) -> bool {
        !sym_name.starts_with('_') && !sym_name.starts_with('#')
    }

    fn is_def_capture_allowed(&self, ctx: &DefCaptureGateContext<'_>) -> bool {
        ctx.sub != Some("constant") || super::is_screaming_snake(ctx.name)
    }

    fn std_member_names(&self) -> &'static [&'static str] {
        RUBY_STD_MEMBERS
    }

    fn ambient_core_names(&self) -> &'static [&'static str] {
        RUBY_AMBIENT_CORE
    }

    fn modules_are_mixins(&self) -> bool {
        true
    }

    fn ambient_shared_names(&self) -> &'static [&'static str] {
        RUBY_AMBIENT_SHARED
    }
    super::impl_grammar_method!(crate tree_sitter_ruby::LANGUAGE, "tags.scm");

    fn extensions(&self) -> &'static [&'static str] {
        &["rb", "rake", "jbuilder"]
    }

    fn std_qualifier_names(&self) -> &'static [&'static str] {
        RUBY_STD_QUALIFIERS
    }
}
