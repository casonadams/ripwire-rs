use ripwire_rs::{Lang, LanguageRegistry};

/// Parses `code` as Rust with the registry's grammar.
pub fn parse_rust(code: &[u8]) -> tree_sitter::Tree {
    let registry = LanguageRegistry::new().unwrap();
    let (language, _) = registry.get(Lang::Rust).unwrap();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(language).unwrap();
    parser.parse(code, None).unwrap()
}
