use crate::domain::Lang;
use std::cell::RefCell;
use tree_sitter::{Language, Parser, Query, QueryCursor};

thread_local! {
    pub static PARSER_STATE: RefCell<(Parser, QueryCursor)> = RefCell::new((
        Parser::new(),
        QueryCursor::new(),
    ));
}

/// Parse `source` with `grammar` on this thread's shared parser.
pub(crate) fn parse_with_grammar(grammar: &Language, source: &[u8]) -> Option<tree_sitter::Tree> {
    PARSER_STATE.with(|state| {
        let mut borrowed = state.borrow_mut();
        let (parser, _) = &mut *borrowed;
        parser.set_language(grammar).ok()?;
        parser.parse(source, None)
    })
}

/// Registry of supported Tree-Sitter languages and compiled queries.
#[derive(Default, Clone, Copy)]
pub struct LanguageRegistry;

impl LanguageRegistry {
    /// Initialize registry with all supported 36 languages.
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self)
    }

    /// Find compiled language and query for given Lang.
    pub fn get(&self, lang: Lang) -> Option<(&Language, &Query)> {
        lang.semantics().grammar_and_query()
    }
}
