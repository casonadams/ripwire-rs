use super::{ComplexityKinds, DialectGroup, LanguageSemantics, SyntaxKinds, VisibilityMode};
use crate::domain::Lang;

pub struct FortranLanguage;
pub static FORTRAN_LANG: FortranLanguage = FortranLanguage;

const FORTRAN_STD_MEMBERS: &[&str] = &[
    "abs",
    "aimag",
    "aint",
    "anint",
    "ceiling",
    "floor",
    "modulo",
    "mod",
    "real",
    "dble",
    "cmplx",
    "int",
    "nint",
    "conjg",
    "dim",
    "max",
    "min",
    "sign",
    "sqrt",
    "exp",
    "log",
    "log10",
    "sin",
    "cos",
    "tan",
    "asin",
    "acos",
    "atan",
    "atan2",
    "sinh",
    "cosh",
    "tanh",
    "matmul",
    "dot_product",
    "transpose",
    "sum",
    "product",
    "all",
    "any",
    "count",
    "maxval",
    "minval",
    "maxloc",
    "minloc",
    "size",
    "shape",
    "lbound",
    "ubound",
    "reshape",
    "trim",
    "adjustl",
    "adjustr",
    "index",
    "scan",
    "verify",
    "len",
    "len_trim",
    "repeat",
    "allocated",
    "associated",
    "present",
    "null",
    "c_loc",
    "c_funloc",
    "c_f_pointer",
    "c_associated",
];

const FORTRAN_STD_QUALIFIERS: &[&str] = &[
    "iso_c_binding",
    "iso_fortran_env",
    "ieee_arithmetic",
    "ieee_exceptions",
    "ieee_features",
    "omp_lib",
];

const KINDS: ComplexityKinds = ComplexityKinds {
    paths: &["case_statement", "type_statement", "rank_statement"],
    flat: &["elseif_clause", "elsewhere_clause"],
    nesting: &[
        "if_statement",
        "arithmetic_if_statement",
        "do_loop",
        "where_statement",
        "forall_statement",
        "conditional_expression",
    ],
    nest_only: &[
        "select_case_statement",
        "select_type_statement",
        "select_rank_statement",
    ],
    else_kinds: &["else_clause"],
    else_bodies: &[],
    jumps: &["keyword_statement", "stop_statement"],
    logical_nodes: &[],
    binary_nodes: &["logical_expression"],
    logical_ops: &[".and.", ".or."],
};

const SYNTAX: SyntaxKinds = SyntaxKinds {
    scope_containers: &["module"],
    body_wrappers: &[
        "variable_declaration",
        "parenthesized_expression",
        "assignment_statement",
    ],
    identifiers: &["identifier", "name"],
    callable_wrappers: &["parenthesized_expression"],
    ..SyntaxKinds::NONE
};

impl LanguageSemantics for FortranLanguage {
    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }

    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }

    fn tag(&self) -> &'static str {
        "fortran"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["f", "for", "f90", "f95", "f03", "f08"]
    }

    super::impl_grammar_method!(crate tree_sitter_fortran::LANGUAGE, "tags.scm");

    fn display_color(&self) -> &'static str {
        "#734f96"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Isolated(Lang::Fortran)
    }
    fn resolve_def_params_and_name(
        &self,
        def_node: Option<tree_sitter::Node<'_>>,
        _source_bytes: &[u8],
        _kind: crate::domain::SymKind,
        orig_name: &str,
    ) -> (crate::gate::contract::ParamArity, String) {
        let kinds = self.syntax_kinds();
        let count = |n| crate::gate::contract::extract_param_arity(n, kinds);
        let arity = def_node
            .and_then(|dn| {
                if dn.kind() == "subroutine" || dn.kind() == "function" {
                    dn.child(0).map(count)
                } else {
                    Some(count(dn))
                }
            })
            .unwrap_or_default();
        (arity, orig_name.to_string())
    }

    fn visibility_mode(&self) -> VisibilityMode {
        VisibilityMode::DefaultPublic
    }

    fn is_symbol_name_public(&self, sym_name: &str) -> bool {
        !sym_name.starts_with('_')
    }

    fn std_member_names(&self) -> &'static [&'static str] {
        FORTRAN_STD_MEMBERS
    }

    fn std_qualifier_names(&self) -> &'static [&'static str] {
        FORTRAN_STD_QUALIFIERS
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
