use super::{ComplexityKinds, DialectGroup, LanguageSemantics, SyntaxKinds, VisibilityMode};
use crate::domain::Lang;

pub struct CobolLanguage;
pub static COBOL_LANG: CobolLanguage = CobolLanguage;

const COBOL_STD_MEMBERS: &[&str] = &[
    "current-date",
    "when-compiled",
    "date-of-integer",
    "day-of-integer",
    "integer-of-date",
    "integer-of-day",
    "numval",
    "numval-c",
    "numval-f",
    "test-numval",
    "test-numval-c",
    "test-numval-f",
    "length",
    "reverse",
    "upper-case",
    "lower-case",
    "max",
    "min",
    "sum",
    "mean",
    "median",
    "range",
    "standard-deviation",
    "variance",
    "random",
    "rem",
    "mod",
    "integer",
    "integer-part",
    "sqrt",
    "log",
    "log10",
    "exp",
    "sin",
    "cos",
    "tan",
    "asin",
    "acos",
    "atan",
    "trim",
    "display",
    "compute",
    "move",
    "perform",
    "call",
];

const COBOL_STD_QUALIFIERS: &[&str] = &[
    "sysin",
    "sysout",
    "syserr",
    "console",
    "standard-1",
    "standard-2",
    "native",
];

// COBOL statements are flat: `if_header`/`else_header` are sibling statement nodes, not wrappers.
const KINDS: ComplexityKinds = ComplexityKinds {
    paths: &["when", "when_other"],
    flat: &["else_if_header"],
    nesting: &["if_header", "perform_statement_loop", "search_statement"],
    nest_only: &["evaluate_header"],
    else_kinds: &["else_header"],
    else_bodies: &[],
    jumps: &[
        "goto_statement",
        "goback_statement",
        "exit_statement",
        "stop_statement",
    ],
    logical_nodes: &[],
    binary_nodes: &["expr"],
    logical_ops: &["AND", "OR"],
};

impl LanguageSemantics for CobolLanguage {
    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }
    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SyntaxKinds::NONE
    }

    fn tag(&self) -> &'static str {
        "cobol"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["cbl", "cob", "cpy"]
    }

    super::impl_grammar_method!(
        ffi tree_sitter_COBOL,
        include_str!("tags.scm")
    );

    fn display_color(&self) -> &'static str {
        "#005a9c"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Isolated(Lang::Cobol)
    }
    fn resolve_def_body_and_end<'a>(
        &self,
        def_node: Option<tree_sitter::Node<'a>>,
        default_end: usize,
    ) -> (Option<tree_sitter::Node<'a>>, usize) {
        let Some(dn) = def_node else {
            return (None, default_end);
        };
        if dn.kind() == "paragraph_header" || dn.kind() == "section_header" {
            let mut curr = dn.next_sibling();
            let mut end = default_end;
            while let Some(sibling) = curr {
                if sibling.kind() == "paragraph_header" || sibling.kind() == "section_header" {
                    end = sibling.start_byte();
                    break;
                }
                end = sibling.end_byte();
                curr = sibling.next_sibling();
            }
            return (Some(dn), end);
        }
        (Some(dn), default_end)
    }
    fn resolve_def_params_and_name(
        &self,
        _def_node: Option<tree_sitter::Node<'_>>,
        _source_bytes: &[u8],
        _kind: crate::domain::SymKind,
        orig_name: &str,
    ) -> (crate::gate::contract::ParamArity, String) {
        let clean = orig_name.trim_end_matches('.').trim();
        (crate::gate::contract::ParamArity::default(), clean.to_string())
    }

    fn visibility_mode(&self) -> VisibilityMode {
        VisibilityMode::DefaultPublic
    }

    fn is_symbol_name_public(&self, _sym_name: &str) -> bool {
        true
    }

    fn std_member_names(&self) -> &'static [&'static str] {
        COBOL_STD_MEMBERS
    }

    fn std_qualifier_names(&self) -> &'static [&'static str] {
        COBOL_STD_QUALIFIERS
    }

    fn is_std_or_builtin_member(&self, name: &str) -> bool {
        self.std_member_names()
            .iter()
            .any(|s| s.eq_ignore_ascii_case(name))
    }

    fn is_std_or_builtin_qualifier(&self, qualifier: &str) -> bool {
        self.std_qualifier_names()
            .iter()
            .any(|s| s.eq_ignore_ascii_case(qualifier))
    }
}
