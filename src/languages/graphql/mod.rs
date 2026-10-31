use super::{ComplexityKinds, DialectGroup, LanguageSemantics, SyntaxKinds};
use crate::domain::Lang;

pub struct GraphqlLanguage;
pub static GRAPHQL_LANG: GraphqlLanguage = GraphqlLanguage;

const SYNTAX: SyntaxKinds = SyntaxKinds {
    identifiers: &["name"],
    ..SyntaxKinds::NONE
};

impl LanguageSemantics for GraphqlLanguage {
    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &ComplexityKinds::NONE
    }

    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }

    fn tag(&self) -> &'static str {
        "gql"
    }
    super::impl_grammar_method!(crate tree_sitter_graphql::LANGUAGE, "tags.scm");

    fn extensions(&self) -> &'static [&'static str] {
        &["graphql", "gql"]
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Isolated(Lang::Graphql)
    }

    fn is_dependency_capable(&self) -> bool {
        false
    }

    fn supports_slice(&self) -> bool {
        false
    }

    /// A named type a field, argument or variable mentions is a dependency on that schema type.
    fn type_mentions_are_edges(&self) -> bool {
        true
    }

    /// The built-in scalars are defined by the spec, never by a schema, so a mention of one is no edge.
    fn is_builtin_keyword(&self, name: &str) -> bool {
        matches!(name, "ID" | "String" | "Int" | "Float" | "Boolean")
    }
}
