mod decorators;
mod macros;
mod records;
mod semantics;
mod unknown;

pub(crate) use decorators::has_registering_decorator;
pub(crate) use macros::{impl_class_dispatch_flags, impl_grammar_method, impl_typed_class_hooks};
pub use records::{
    CallableBinding, DefCaptureGateContext, DialectGroup, Implementation, VisibilityMode,
};
pub use semantics::LanguageSemantics;
pub use unknown::{UNKNOWN_LANG, UnknownLanguage};

pub mod common;
pub use common::util::*;
pub use common::{
    ComplexityKinds, ForeignExport, LintGroup, SyntaxKinds, flags, reads, source, typed,
};
pub(crate) use common::{lint, lint_hooks};

/// The language registry: one line per language plugin (`src/languages/<module>/mod.rs`).
/// Generates the `Lang` enum, its module declarations and the semantics lookup; variant order is serialized.
macro_rules! register_languages {
    ($($variant:ident => $module:ident :: $instance:ident,)*) => {
        $(pub mod $module;)*

        #[repr(u8)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        pub enum Lang {
            $($variant,)*
            Unknown,
        }

        const LANG_COUNT: usize = [$(stringify!($variant)),*].len();

        pub const ALL_LANGS: [Lang; LANG_COUNT] = [$(Lang::$variant),*];

        static SEMANTICS_TABLE: [&dyn LanguageSemantics; LANG_COUNT + 1] =
            [$(&$module::$instance,)* &UNKNOWN_LANG];
    };
}

register_languages! {
    Rust => rust::RUST_LANG,
    TypeScript => typescript::TYPESCRIPT_LANG,
    JavaScript => javascript::JAVASCRIPT_LANG,
    Python => python::PYTHON_LANG,
    Go => go::GO_LANG,
    C => c::C_LANG,
    Cpp => cpp::CPP_LANG,
    Cuda => cuda::CUDA_LANG,
    Metal => metal::METAL_LANG,
    Java => java::JAVA_LANG,
    CSharp => csharp::CSHARP_LANG,
    Swift => swift::SWIFT_LANG,
    Bash => bash::BASH_LANG,
    Ruby => ruby::RUBY_LANG,
    Php => php::PHP_LANG,
    Lua => lua::LUA_LANG,
    Elixir => elixir::ELIXIR_LANG,
    Dart => dart::DART_LANG,
    Kotlin => kotlin::KOTLIN_LANG,
    GDScript => gdscript::GDSCRIPT_LANG,
    Json => json::JSON_LANG,
    Toml => toml::TOML_LANG,
    Yaml => yaml::YAML_LANG,
    Markdown => markdown::MARKDOWN_LANG,
    Zig => zig::ZIG_LANG,
    Solidity => solidity::SOLIDITY_LANG,
    Hcl => hcl::HCL_LANG,
    Scala => scala::SCALA_LANG,
    Erlang => erlang::ERLANG_LANG,
    Sql => sql::SQL_LANG,
    Proto => proto::PROTO_LANG,
    Graphql => graphql::GRAPHQL_LANG,
    ObjC => objc::OBJC_LANG,
    BrightScript => brightscript::BRIGHTSCRIPT_LANG,
    Fortran => fortran::FORTRAN_LANG,
    Cobol => cobol::COBOL_LANG,
}

impl Lang {
    /// Return the static `LanguageSemantics` implementation for this language.
    #[inline]
    #[must_use]
    pub fn semantics(self) -> &'static dyn LanguageSemantics {
        SEMANTICS_TABLE[self as usize]
    }
}
