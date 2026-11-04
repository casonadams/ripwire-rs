use super::{ComplexityKinds, DialectGroup, Lang, LanguageSemantics, SyntaxKinds};

pub struct UnknownLanguage;
pub static UNKNOWN_LANG: UnknownLanguage = UnknownLanguage;

impl LanguageSemantics for UnknownLanguage {
    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &ComplexityKinds::NONE
    }

    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SyntaxKinds::NONE
    }

    fn tag(&self) -> &'static str {
        "?"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Isolated(Lang::Unknown)
    }

    fn is_dependency_capable(&self) -> bool {
        false
    }

    fn supports_slice(&self) -> bool {
        false
    }

    fn supports_test_partners(&self) -> bool {
        false
    }
}
