use super::Lang;
use crate::domain::SymKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallableBinding {
    pub owner: String,
    pub field: String,
    pub target: Option<String>,
    pub start_byte: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Implementation {
    pub subject: String,
    pub trait_path: Option<String>,
    pub subject_bound: Option<String>,
    pub generic_subject: bool,
    pub start_byte: usize,
    pub end_byte: usize,
}

/// Cross-language symbol resolution compatibility group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialectGroup {
    CFamily,
    Web,
    Jvm,
    Beam,
    Markup,
    Isolated(Lang),
}

/// How a language determines whether a symbol declaration is public.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibilityMode {
    /// Requires explicit `pub` or `public` modifier on the AST node (Rust, Java, C#, Kotlin, PHP, Swift).
    ExplicitModifier,
    /// Determined by symbol name convention (Go uppercase initial, Python/Dart/Ruby/Elixir/Lua/Bash/GDScript leading `_`).
    ByNameConvention,
    /// Public unless leading `_` on identifier.
    DefaultPublic,
}

/// Context passed to `LanguageSemantics::is_def_capture_allowed` to filter raw query captures.
#[derive(Clone, Copy)]
pub struct DefCaptureGateContext<'a> {
    pub kind: SymKind,
    pub sub: Option<&'static str>,
    pub name: &'a str,
    pub name_node: Option<tree_sitter::Node<'a>>,
    pub def_node: Option<tree_sitter::Node<'a>>,
    pub source_bytes: &'a [u8],
}
