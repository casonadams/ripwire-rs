pub mod dataform_vars;
pub mod embedded;
pub mod sqlx;

use super::{ComplexityKinds, DialectGroup, LanguageSemantics, SyntaxKinds};
use crate::domain::Lang;

pub struct SqlLanguage;
pub static SQL_LANG: SqlLanguage = SqlLanguage;

const SYNTAX: SyntaxKinds = SyntaxKinds {
    body_blocks: &["function_body", "block"],
    body_wrappers: &["parenthesized_expression"],
    function_defs: &["function_declaration"],
    non_local_declarations: &["function_declaration"],
    identifiers: &["identifier"],
    callable_wrappers: &["parenthesized_expression"],
    ..SyntaxKinds::NONE
};

impl LanguageSemantics for SqlLanguage {
    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &ComplexityKinds::NONE
    }

    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }

    fn tag(&self) -> &'static str {
        "sql"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["sql", "sqlx"]
    }

    fn parse_file_override(
        &self,
        source_bytes: &[u8],
        rel_path: &str,
        _language: &tree_sitter::Language,
        _query: &tree_sitter::Query,
    ) -> Option<crate::ingest::FileParseResult> {
        rel_path
            .ends_with(".sqlx")
            .then(|| sqlx::parse_sqlx(source_bytes, rel_path))
    }
    super::impl_grammar_method!(ffi tree_sitter_sql, include_str!("tags.scm"));

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Isolated(Lang::Sql)
    }

    fn is_dependency_capable(&self) -> bool {
        false
    }

    fn supports_slice(&self) -> bool {
        false
    }

    fn embedded_refs(
        &self,
        root: tree_sitter::Node<'_>,
        source: &[u8],
        refs: &mut Vec<crate::ingest::RawRef>,
    ) {
        if embedded::has_dml_anchor(source) {
            embedded::extract_refs(root, source, refs);
        }
    }

    fn supports_embedded_sql(&self) -> bool {
        false
    }

    fn is_table_read_write_target(&self) -> bool {
        true
    }
}
