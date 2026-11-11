use super::records::{
    CallableBinding, DefCaptureGateContext, DialectGroup, Implementation, VisibilityMode,
};
use super::{
    ComplexityKinds, ForeignExport, Lang, LintGroup, SyntaxKinds, check_leading_underscore_private,
    common, find_enclosing_definition_node, has_any_visibility_modifier, is_non_entrypoint_path,
    lint_hooks, reads, resolve_relative_or_exact, source, strip_import_quotes,
};
use crate::domain::SymKind;
use crate::ingest::FileParseResult;
use std::collections::HashMap;
use tree_sitter::{Language, Query};

/// Whether `node` names what its parent defines: the parent's `name` field, or the `declarator` of a
/// C-family function declarator (a prototype or a definition).
fn is_definition_name(node: tree_sitter::Node<'_>) -> bool {
    node.parent().is_some_and(|parent| {
        parent.child_by_field_name("name") == Some(node)
            || (parent.kind() == "function_declarator"
                && parent.child_by_field_name("declarator") == Some(node))
    })
}

/// Unified trait encapsulating all language-specific syntax, AST, visibility, and resolution rules.
pub trait LanguageSemantics: Sync {
    /// Short canonical tag used in XML output (`"rs"`, `"ts"`, etc.).
    fn tag(&self) -> &'static str;

    /// Grammar node kinds the language-neutral extraction, slicing, and contract passes use.
    fn syntax_kinds(&self) -> &'static SyntaxKinds;

    /// Receiver of a member access whose grammar keeps it outside the access node (sibling selectors).
    fn member_receiver<'a>(&self, _access: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
        None
    }

    /// Receiver spellings that name the enclosing type's own instance.
    fn self_receivers(&self) -> &'static [&'static str] {
        &["self", "this", "Self"]
    }
    fn receiver_text<'a>(&self, text: &'a str) -> std::borrow::Cow<'a, str> {
        std::borrow::Cow::Borrowed(text)
    }
    fn call_receiver_name<'a>(
        &self,
        receiver: tree_sitter::Node<'_>,
        source: &'a [u8],
    ) -> Option<&'a str> {
        if receiver.kind() != "call_expression" {
            return None;
        }
        let callee = receiver
            .child_by_field_name("function")
            .filter(|callee| callee.kind() == "identifier")?;
        callee.utf8_text(source).ok()
    }
    fn constructed_receiver<'a>(
        &self,
        _receiver: tree_sitter::Node<'_>,
        _source: &'a [u8],
    ) -> Option<&'a str> {
        None
    }
    /// The receiver text of a call whose receiver is itself a call chain or a cast: `a.b().c()` names
    /// `a.b()`, and `((T) x).c()` names `(T)`. The text is read by `receiver_type_at`.
    fn receiver_chain(&self, _receiver: tree_sitter::Node<'_>, _source: &[u8]) -> Option<String> {
        None
    }
    /// True where an import reference's qualifier is the package its name is imported from
    /// (`import a.b.C;` names `C` with qualifier `a.b`), so the import's full path is `qualifier.name`.
    fn import_names_full_path(&self) -> bool {
        false
    }
    fn has_class_inheritance(&self) -> bool {
        false
    }
    fn overloads_by_arity(&self) -> bool {
        false
    }
    /// Rust: impl members are not in lexical scope; a bare `f()` needs `self.` or `Self::`.
    fn bare_calls_skip_type_members(&self) -> bool {
        false
    }
    /// True where a scoped call keeps its whole module path as the qualifier (`a::b::f()` -> `a::b`), not just the last segment.
    fn keeps_path_qualifier(&self) -> bool {
        false
    }
    /// True where a type a module declares is named through the module that imports it (Zig `ast.Type.init()`
    /// after `const ast = @import("ast.zig")`): the call's qualifier is `ast::Type`, narrowed to that file.
    fn module_member_qualifiers(&self) -> bool {
        false
    }
    /// Path suffixes of the files that root a crate/package module tree (Rust `src/lib.rs`).
    fn crate_root_suffixes(&self) -> &'static [&'static str] {
        &[]
    }
    /// True where the reference name is the field in a call of a parenthesized field read (Rust `(self.f)(x)`).
    fn is_callable_field_access(&self, _name_node: tree_sitter::Node<'_>) -> bool {
        false
    }
    /// True if a type mention (`RefRole::Type`, e.g. a Rust struct literal `Name { .. }`) is a graph edge to the
    /// named type. Off by default: most languages' type mentions are declarations or noise.
    fn type_mentions_are_edges(&self) -> bool {
        false
    }

    /// True if this language writes bodyless declarations of entities defined elsewhere
    /// (C prototypes, TypeScript overload signatures).
    fn has_declarations(&self) -> bool {
        false
    }

    /// True if bodyless `decl` declares the same entity as same-named, bodied `def`, so both
    /// form one definition group. Consulted only when `has_declarations` holds.
    fn declaration_ties(
        &self,
        _decl: &crate::domain::Symbol,
        _def: &crate::domain::Symbol,
    ) -> bool {
        false
    }

    /// True where a bodyless declaration (a TypeScript interface member) is a contract its
    /// implementations satisfy by shape, not a definition that shares their name: by-name calls
    /// reach a same-named implementation instead of the contract.
    fn declarations_yield_to_definitions(&self) -> bool {
        false
    }

    /// True if scope `spec` names a specialization of the generic scope `primary` (C++ `Box<int>`).
    fn is_specialization_of(&self, _spec: &str, _primary: &str) -> bool {
        false
    }

    /// Definitions in this file that a binding framework exposes to another language.
    fn foreign_exports(&self, _root: tree_sitter::Node<'_>, _source: &[u8]) -> Vec<ForeignExport> {
        Vec::new()
    }

    /// File extensions mapped to this language (defaults to `&[self.tag()]` when empty).
    fn extensions(&self) -> &'static [&'static str] {
        &[]
    }

    /// True if `ext` (case-insensitively) maps to this language.
    fn matches_extension(&self, ext: &str) -> bool {
        let exts = self.extensions();
        if exts.is_empty() {
            self.tag() != "?" && ext.eq_ignore_ascii_case(self.tag())
        } else {
            exts.iter().any(|e| ext.eq_ignore_ascii_case(e))
        }
    }

    /// Compiled Tree-sitter grammar and tags query for this language.
    fn grammar_and_query(&self) -> Option<(&'static Language, &'static Query)> {
        None
    }

    /// Grammar node kinds this language contributes to `cx` / `ccx` complexity.
    fn complexity_kinds(&self) -> &'static ComplexityKinds;

    /// A call qualifier the generic receiver extraction cannot see (e.g. Dart's sibling selector chains).
    fn call_qualifier(
        &self,
        _name_node: tree_sitter::Node<'_>,
        _source_bytes: &[u8],
    ) -> Option<String> {
        None
    }

    /// True if a type-qualified call whose qualifier names no indexed definition is external, not a
    /// candidate for same-named members of unrelated types.
    fn qualifier_must_be_indexed(&self) -> bool {
        false
    }

    /// True if `'` can introduce a lifetime identifier (`'a`) rather than a character literal.
    fn has_lifetime_tick_syntax(&self) -> bool {
        false
    }

    fn source_region(&self, kind: &str) -> Option<source::SourceRegion> {
        source::node_region(kind)
    }
    /// Visualization fill color for HTML call-graph export.
    fn display_color(&self) -> &'static str {
        "#888888"
    }

    /// Cross-language resolution dialect group.
    fn dialect_group(&self) -> DialectGroup;

    /// True if syntax-backed import/include resolution is supported for dependency metrics.
    fn is_dependency_capable(&self) -> bool {
        true
    }

    /// True if `#` introduces a line comment in token stream clone detection.
    fn uses_hash_comments(&self) -> bool {
        false
    }

    /// True if this language's tags query captures read sites, so `--uses` can list them.
    fn indexes_reads(&self) -> bool {
        self.grammar_and_query().is_some_and(|(_, q)| {
            q.capture_names()
                .iter()
                .any(|c| matches!(*c, "reference.read" | "reference.attribute"))
        })
    }

    /// Reject declaration targets, labels, types and callees from broad read captures.
    fn is_read_capture_allowed(
        &self,
        _node: tree_sitter::Node<'_>,
        _ancestors: &[tree_sitter::Node<'_>],
        _source: &[u8],
    ) -> bool {
        true
    }

    /// Reject write captures whose identifier is not a real write target (a method call that does not mutate).
    fn is_write_capture_allowed(
        &self,
        _node: tree_sitter::Node<'_>,
        _ancestors: &[tree_sitter::Node<'_>],
        _source: &[u8],
    ) -> bool {
        true
    }

    /// Declared result types of the free functions a file declares, as `(name, result type)`. The project pass
    /// types the locals that take a function's result; empty unless the language does so.
    fn function_return_types(
        &self,
        _root: tree_sitter::Node<'_>,
        _source: &[u8],
    ) -> Vec<(String, String)> {
        Vec::new()
    }

    /// Lexical locals must not resolve to unrelated same-named repository definitions.
    fn read_bindings<'a>(
        &self,
        _root: tree_sitter::Node<'_>,
        _source: &'a [u8],
    ) -> reads::ReadBindings<'a> {
        reads::ReadBindings::default()
    }

    fn callable_bindings(
        &self,
        _root: tree_sitter::Node<'_>,
        _source: &[u8],
    ) -> Vec<CallableBinding> {
        Vec::new()
    }

    /// Impl blocks with exact byte spans, used to attribute body methods to their subject and trait.
    fn implementations(&self, _root: tree_sitter::Node<'_>, _source: &[u8]) -> Vec<Implementation> {
        Vec::new()
    }

    /// `(bound name, module path)` for each imported name that may be a submodule (`from pkg import mod`),
    /// so qualified calls through the name can bind into that module's file.
    fn submodule_imports(
        &self,
        _root: tree_sitter::Node<'_>,
        _source: &[u8],
    ) -> Vec<(String, String)> {
        Vec::new()
    }

    /// Module paths a file loads with a dynamic `import("path")`, whose exports the importer reaches without
    /// naming them.
    fn dynamic_imports(&self, _root: tree_sitter::Node<'_>, _source: &[u8]) -> Vec<String> {
        Vec::new()
    }

    /// `(name, scope start byte, scope end byte)` for each local variable or parameter a file binds, for languages where such a binding shadows a
    /// same-named definition elsewhere (`local_definition_shadows_name`).
    fn local_bindings(
        &self,
        _root: tree_sitter::Node<'_>,
        _source: &[u8],
    ) -> Vec<(String, u32, u32)> {
        Vec::new()
    }

    /// Parser of this language's aggregate bodies into sized, offset fields for `--field-affinity=STRUCT`; `None` where unmodelled.
    fn layout_model(&self) -> Option<fn(&str) -> Vec<crate::metrics::FieldLayout>> {
        None
    }

    /// True if `--field-affinity=STRUCT` byte-layout modeling is supported.
    fn supports_c_layout(&self) -> bool {
        self.layout_model().is_some()
    }

    /// True if `--slice` intra-procedural def-use slicing is supported.
    fn supports_slice(&self) -> bool {
        true
    }

    /// True if string literals in this language should be scanned for embedded SQL DML/DDL table references.
    fn supports_embedded_sql(&self) -> bool {
        true
    }

    /// References this language finds inside other languages' string literals (SQL table access).
    fn embedded_refs(
        &self,
        _root: tree_sitter::Node<'_>,
        _source: &[u8],
        _refs: &mut Vec<crate::ingest::RawRef>,
    ) {
    }

    /// Import references this language spells as statements or calls the tags query cannot bind to the names
    /// they introduce (CommonJS `const X = require("p")`). Appends one `Import` reference per bound name, named
    /// `"p as X"` (see `import_binding`), or one `"p"` reference when nothing is bound.
    fn import_refs(
        &self,
        _root: tree_sitter::Node<'_>,
        _source: &[u8],
        _refs: &mut Vec<crate::ingest::RawRef>,
    ) {
    }

    /// Environment-read spellings this language uses (`getenv("X")`, `process.env.X`).
    fn env_reads(&self) -> &'static [common::flags::EnvRead] {
        &[]
    }

    /// Build manifests, outside this language's own sources, that hold its feature flags (CMake files for C++).
    fn flag_manifests(&self) -> &'static [&'static str] {
        &[]
    }

    /// Feature-flag facts in a source file (or one of `flag_manifests`).
    fn flag_facts(&self, lines: &[&str], _path: &str) -> Vec<common::flags::FlagFact> {
        common::flags::env_facts(
            lines,
            self.uses_hash_comments(),
            self.has_lifetime_tick_syntax(),
            self.env_reads(),
        )
    }

    /// True if files of this language are prose documents (excluded from code-identifier sets).
    fn is_prose_document(&self) -> bool {
        false
    }

    /// True when name resolution is trusted to exclude same-named definitions: a bare name written in a
    /// file that defines or binds it means that definition, and a reference bound by import or type
    /// evidence reaches only its target.
    fn local_definition_shadows_name(&self) -> bool {
        false
    }

    /// True when this language's `Section` symbols are module declarations (`mod name;`) that an exact-name
    /// query anchors on like a code symbol. Markdown and YAML sections are not anchors.
    fn module_declarations_are_anchors(&self) -> bool {
        false
    }

    /// True if files of this language are structured data (JSON, TOML, YAML), excluded from code inventory.
    fn is_data_format(&self) -> bool {
        false
    }

    /// True if `path` is a public interface header (C-family `.h`, `.hpp`, ...).
    fn is_header_file(&self, _path: &str) -> bool {
        false
    }

    /// True if definitions in this language represent SQL tables targeted by `RefRole::Read` / `RefRole::Write`.
    fn is_table_read_write_target(&self) -> bool {
        false
    }

    /// How visibility is determined when comparing HEAD vs working-tree contracts.
    fn visibility_mode(&self) -> VisibilityMode {
        VisibilityMode::DefaultPublic
    }

    /// Maximum byte size allowed for indexing a file of this language.
    fn max_file_size(&self, default_max: u64) -> u64 {
        default_max
    }

    /// Refine file dialect from source content when the extension is shared (e.g., `.h` across C, C++, and Objective-C).
    fn refine_dialect(&self, base_lang: Lang, _rel_path: &str, _source_bytes: &[u8]) -> Lang {
        base_lang
    }

    /// True if a symbol represents a conventional program entrypoint (`main`, `__main__.py`, etc.) for `<inv><entry>`.
    fn is_entrypoint_symbol(
        &self,
        sym_name: &str,
        scope: Option<&str>,
        kind: SymKind,
        rel_path: &str,
    ) -> bool {
        if !kind.is_callable() || is_non_entrypoint_path(rel_path) {
            return false;
        }
        sym_name == "main" && scope.is_none()
    }

    /// Optional custom file parser override (used by Markdown, YAML nesting guard, and Astro frontmatter).
    fn parse_file_override(
        &self,
        _source_bytes: &[u8],
        _rel_path: &str,
        _language: &Language,
        _query: &Query,
    ) -> Option<FileParseResult> {
        None
    }

    /// Byte offsets `(declaration, doc)` where a definition's span starts. The declaration covers keywords its
    /// defining node leaves out (a TS/JS `export`); `doc` is where the doc comment attached above it begins.
    fn resolve_def_span_start(
        &self,
        def_node: tree_sitter::Node<'_>,
        _source_bytes: &[u8],
    ) -> (usize, usize) {
        (def_node.start_byte(), def_node.start_byte())
    }

    /// Resolve the AST node containing the definition body and its ending byte offset.
    fn resolve_def_body_and_end<'a>(
        &self,
        def_node: Option<tree_sitter::Node<'a>>,
        default_end: usize,
    ) -> (Option<tree_sitter::Node<'a>>, usize) {
        let Some(dn) = def_node else {
            return (None, default_end);
        };
        if let Some(parent) = find_enclosing_definition_node(dn) {
            return (Some(parent), parent.end_byte());
        }
        (Some(dn), default_end)
    }

    /// Extract parameter count and format canonical symbol name (e.g., Elixir `name/arity`).
    fn resolve_def_params_and_name(
        &self,
        def_node: Option<tree_sitter::Node<'_>>,
        _source_bytes: &[u8],
        _kind: SymKind,
        orig_name: &str,
    ) -> (crate::gate::contract::ParamArity, String) {
        let params = def_node
            .map(|n| crate::gate::contract::extract_param_arity(n, self.syntax_kinds()))
            .unwrap_or_default();
        (params, orig_name.to_string())
    }

    /// Format a call reference's target name (e.g., Elixir `name/arity`).
    fn format_callee_name(&self, name: &str, _arg_count: u16, _arg_count_known: bool) -> String {
        name.to_string()
    }

    fn supports_internal_linkage(&self) -> bool {
        false
    }

    fn is_internal_function(
        &self,
        _root: tree_sitter::Node<'_>,
        _name_byte: usize,
        _source: &[u8],
    ) -> bool {
        false
    }

    /// Calls `visit` with the text of every identifier token in `root` that uses a name rather than
    /// defining it (each leaf whose kind ends in `identifier`, outside a definition's `name` and a
    /// function declarator's `declarator`, so a prototype is not a use). Dead-code checks count these
    /// as name-use evidence, so value positions (shorthand properties, fn pointers, macro arguments,
    /// re-exports) count too. `visit` gets each identifier's text and start byte.
    fn for_each_identifier(
        &self,
        root: tree_sitter::Node<'_>,
        source: &[u8],
        visit: &mut dyn FnMut(&str, usize),
    ) {
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.child_count() > 0 {
                let mut cursor = node.walk();
                stack.extend(node.children(&mut cursor));
            } else if node.kind().ends_with("identifier")
                && !is_definition_name(node)
                && let Ok(text) = node.utf8_text(source)
            {
                visit(text, node.start_byte());
            }
        }
    }

    /// Optional language-specific enclosing scope extractor for non-standard container AST shapes.
    fn extract_custom_scope(
        &self,
        _parent: tree_sitter::Node<'_>,
        _name_node: tree_sitter::Node<'_>,
        _source_bytes: &[u8],
    ) -> Option<String> {
        None
    }

    /// True if a captured identifier is a language keyword or cast operator that must not be indexed.
    fn is_builtin_keyword(&self, _name: &str) -> bool {
        false
    }

    /// True if a captured call reference is a language builtin/macro that should not form a user call edge.
    fn should_ignore_call(
        &self,
        _name_node: Option<tree_sitter::Node<'_>>,
        _ancestors: &[tree_sitter::Node<'_>],
        name: &str,
    ) -> bool {
        self.is_builtin_keyword(name)
    }

    /// True if this node is a value inside a macro argument, where a same-named local binding shadows a call.
    fn is_macro_argument(
        &self,
        _node: tree_sitter::Node<'_>,
        _ancestors: &[tree_sitter::Node<'_>],
    ) -> bool {
        false
    }

    /// The struct type a call builds the base of, for `S { f: 1, ..Default::default() }`: the call binds to
    /// `S`'s own `Default` impl. None for any other call.
    fn struct_update_base_type(
        &self,
        _name_node: tree_sitter::Node<'_>,
        _source: &[u8],
    ) -> Option<String> {
        None
    }

    /// The method a type's own impl provides to build it with no arguments (Rust's `Default::default`). None
    /// where the language has no such convention.
    fn default_constructor_name(&self) -> Option<&'static str> {
        None
    }

    /// Standard library / builtin member names declined on untyped receivers.
    fn std_member_names(&self) -> &'static [&'static str] {
        &[]
    }

    /// Standard library / builtin property names: a member read of one binds to an in-tree
    /// accessor only with receiver evidence.
    fn std_property_names(&self) -> &'static [&'static str] {
        &[]
    }

    /// Standard library / runtime namespace qualifiers classified as `External`.
    fn std_qualifier_names(&self) -> &'static [&'static str] {
        &[]
    }

    /// The kind a definition captured as `kind` takes in this language. Dart: an `abstract class` is an interface.
    fn refine_def_kind(
        &self,
        kind: SymKind,
        _def_node: Option<tree_sitter::Node<'_>>,
        _source: &[u8],
    ) -> SymKind {
        kind
    }

    /// True if `name`, called on a receiver whose type is unknown, names a core library member of this language
    /// (declined rather than bound by name to an unrelated in-tree member).
    fn is_untyped_core_member(&self, _name: &str) -> bool {
        false
    }

    /// Library functions whose bodyless free declarations in the tree name the library's definition,
    /// not an in-tree target: a prototype of `malloc` in a header is the C library's `malloc`.
    fn library_function_names(&self) -> &'static [&'static str] {
        &[]
    }

    /// True when a class's field types, and its type aliases, are resolved across files in the tree: a member
    /// used in a file that does not declare its class (an out-of-line `-inl.h` body) takes the declared type.
    fn cross_file_field_types(&self) -> bool {
        false
    }

    /// True where a bodyless class or struct declaration (`class logger;`) is a forward declaration of the type a
    /// definition elsewhere gives a body: it declares no members and never competes with that definition by name.
    fn bodyless_types_are_forward_declarations(&self) -> bool {
        false
    }

    /// True where a nested type is named by the type that encloses it (`Outer.Inner`): its owner key is qualified by
    /// that type, and a bare name written inside the enclosing type names the nested one first.
    fn nested_types_are_scoped(&self) -> bool {
        false
    }

    /// Core members of the language's standard smart pointers, optionals and `swap`. A call of one on a receiver with
    /// no type evidence, or on a smart pointer, names the standard member, not an in-tree class's method.
    fn core_member_names(&self) -> &'static [&'static str] {
        &[]
    }

    /// Type aliases a file declares, as `(alias, declared target type)`.
    fn type_aliases(&self, _root: tree_sitter::Node<'_>, _source: &[u8]) -> Vec<(String, String)> {
        Vec::new()
    }

    /// True if a method name belongs to the language's standard container/string/builtin types (declined on untyped receivers).
    fn is_std_or_builtin_member(&self, name: &str) -> bool {
        self.std_member_names().contains(&name)
    }

    /// True if a method name is the language's own standard or builtin member, which `--external-surface` leaves out.
    /// Dependency members, such as a parser crate's methods, are not language builtins and stay external.
    fn is_language_builtin_member(&self, name: &str) -> bool {
        self.is_std_or_builtin_member(name)
    }

    /// True if an explicit call qualifier names a standard library or builtin runtime namespace/type.
    fn is_std_or_builtin_qualifier(&self, qualifier: &str) -> bool {
        self.std_qualifier_names().contains(&qualifier)
    }

    /// Optional byte-preserving macro-blanking pass invoked only when the primary tree-sitter parse contains syntax errors.
    fn blank_error_macros(&self, _source_bytes: &[u8]) -> Option<Vec<u8>> {
        None
    }

    /// True if a captured definition satisfies language-specific structural/naming gates.
    fn is_def_capture_allowed(&self, _ctx: &DefCaptureGateContext<'_>) -> bool {
        true
    }

    /// How a captured `Var` binding may change. Only `Frozen` (an immutable value) and `Const` (a binding
    /// whose object contents are mutable only as evidenced by writes) differ from the reassignable default.
    fn var_binding(&self, _ctx: &DefCaptureGateContext<'_>) -> crate::domain::Binding {
        crate::domain::Binding::Mutable
    }

    /// True if a captured definition is a bodyless member an interface declares by contract (a
    /// TypeScript function-typed interface property): typed dispatch reaches it, a by-name call never does.
    fn is_contract_member(&self, _ctx: &DefCaptureGateContext<'_>) -> bool {
        false
    }

    /// A getter or property, run by reading it as a member.
    fn def_is_accessor(&self, _ctx: &DefCaptureGateContext<'_>) -> bool {
        false
    }

    /// Registered by a decorator that hands it to a framework (a route, a hook, a task).
    fn def_is_registered(&self, _ctx: &DefCaptureGateContext<'_>) -> bool {
        false
    }

    /// A constructor or factory declared in a class (only the class name reaches it).
    fn def_is_constructor(&self, _ctx: &DefCaptureGateContext<'_>) -> bool {
        false
    }

    fn def_takes_self(&self, _ctx: &DefCaptureGateContext<'_>) -> bool {
        false
    }

    fn member_call_needs_self(&self) -> bool {
        false
    }

    /// True if a symbol name is public by language naming convention.
    fn is_symbol_name_public(&self, sym_name: &str) -> bool {
        !sym_name.starts_with('_')
    }

    /// File stems that hold only tests by this language's module convention (e.g. Rust `tests.rs`).
    fn is_test_module_stem(&self, _file_stem: &str) -> bool {
        false
    }

    /// True when partner tests (a test named after a changed file) apply to this language's files.
    fn supports_test_partners(&self) -> bool {
        true
    }

    /// The subject a test file stem names by this language's convention; the universal rules by default.
    fn test_partner_subject<'a>(&self, file_stem: &'a str, in_test_dir: bool) -> Option<&'a str> {
        common::util::universal_test_partner_subject(file_stem, in_test_dir)
    }

    /// True for shell scripts: test files that run programs as subprocesses the call graph cannot see.
    fn is_script_runner(&self) -> bool {
        false
    }

    /// True for standalone assertion scripts (`x.check.ts`): tests the call graph reaches only through imports.
    fn is_assertion_script(&self, _path: &str) -> bool {
        false
    }

    /// True when a member call `x.f()` cannot reach a non-callable variable named `f` (a property call never
    /// binds to a same-named module or local binding).
    fn member_call_targets_callables_only(&self) -> bool {
        false
    }

    /// True where a member call `x.f()` cannot reach a free function named `f` (Dart: a function is never a member,
    /// so only methods and fields answer `x.f()`).
    fn member_call_targets_members_only(&self) -> bool {
        false
    }

    /// True where a member call `x.f()` never names a type: a struct or interface declaration is no method, so a
    /// call on an untyped receiver does not bind to a same-named type (Go `r.Context()` is not `type Context`).
    fn member_calls_skip_types(&self) -> bool {
        false
    }

    /// Multiplier for this language's symbols in task retrieval; below 1 for declaration-only formats.
    fn retrieval_weight(&self) -> f64 {
        1.0
    }

    /// File names this language's code generators emit by convention (e.g. Go `x.pb.go`); never indexed.
    fn is_generated_file_name(&self, _file_name: &str) -> bool {
        false
    }

    /// Manifest whose nearest ancestor copy selects this language's test runner (e.g. `package.json`).
    fn test_runner_manifest(&self) -> Option<&'static str> {
        None
    }

    /// Command that runs the test file at repo-relative `path`, from its source and nearest manifest text.
    fn test_run_command(
        &self,
        _path: &str,
        _source: &str,
        _manifest: Option<&str>,
    ) -> Option<String> {
        None
    }

    /// Terminator of a literal (quoted) heredoc that `line` opens; the body lines until it are data, not code.
    fn quoted_heredoc_terminator<'a>(&self, _line: &'a str) -> Option<&'a str> {
        None
    }

    /// True if an AST definition node has public visibility in this language.
    fn check_node_visibility(&self, def_node: tree_sitter::Node<'_>, source_bytes: &[u8]) -> bool {
        match self.visibility_mode() {
            VisibilityMode::ExplicitModifier => has_any_visibility_modifier(def_node, source_bytes),
            VisibilityMode::ByNameConvention => {
                check_leading_underscore_private(def_node, source_bytes, self.syntax_kinds())
            }
            VisibilityMode::DefaultPublic => true,
        }
    }

    /// Import specifier implied by a module declaration symbol (e.g. Rust `mod foo;` -> `self::foo`).
    fn symbol_implied_import(&self, _kind: SymKind, _name: &str) -> Option<String> {
        None
    }
    /// True where `impl Trait for Type` blocks bind methods to types and traits dispatch to them (Rust).
    fn has_trait_impls(&self) -> bool {
        false
    }
    /// True where declared implementations (Rust `impl`, TypeScript `class … implements`) feed the trait index.
    fn indexes_implementations(&self) -> bool {
        self.has_trait_impls()
    }
    /// True where a method's receiver type is always declared in the tree and any named type (`type T func()`, a
    /// map or a slice) can carry methods, so a method whose receiver names no indexed type binds to none (Go).
    fn methods_declare_their_receiver(&self) -> bool {
        false
    }
    /// True where a named type that is neither a class, struct nor interface (`Other`) is a type a method can name.
    fn named_types_take_methods(&self) -> bool {
        false
    }
    /// True where a function scoped to a type is that type's method although it is tagged as a plain
    /// function (Python tags every `def`, so a class body's `def` is a `Function`). Such functions join
    /// the trait index's method lists.
    fn functions_are_type_members(&self) -> bool {
        false
    }
    /// True where a module is a mixin type: its methods join the trait index and a class that includes it
    /// reaches them (Ruby `module`, tagged as a section, is otherwise not a nominal type).
    fn modules_are_mixins(&self) -> bool {
        false
    }
    /// Core runtime method names (Object, Kernel, IO, String, Array, Hash) that an untyped receiver or an
    /// unqualified call may mean. A name-only binding of such a call needs in-tree evidence for its owner.
    fn ambient_core_names(&self) -> &'static [&'static str] {
        &[]
    }
    /// Names that user classes also define (`call`, `each`): an untyped call binds only within the caller's
    /// own type chain or file, never by name across the whole tree.
    fn ambient_shared_names(&self) -> &'static [&'static str] {
        &[]
    }
    /// True where a receiver whose declared type text is not an indexed type may use name resolution.
    fn unindexed_receiver_type_uses_names(&self, _receiver_type: &str) -> bool {
        false
    }
    /// Dart: `extends` and mixins are not indexed, so a miss on a typed receiver proves nothing.
    fn typed_receiver_miss_uses_names(&self) -> bool {
        false
    }
    /// True where an untyped member call whose name the caller's own class also declares declines instead of
    /// binding to that class: a standard or core member name, or a member another in-tree class declares too.
    fn untyped_member_declines_caller_class(&self) -> bool {
        false
    }
    /// True where a call receiver whose result type is not declared (`make().m()`) declines instead of name resolution.
    fn undeclared_call_receiver_declines(&self) -> bool {
        false
    }
    /// Declared type text of member `member` of a receiver whose declared type text is `receiver_type`.
    fn member_receiver_type(&self, _receiver_type: &str, _member: &str) -> Option<String> {
        None
    }
    /// True where a call through an interface also reaches the interface's own declaration of the method.
    fn interface_calls_keep_declaration(&self) -> bool {
        false
    }
    /// True where interfaces are satisfied by shape, so a class conforms without declaring it (TypeScript).
    fn interfaces_are_structural(&self) -> bool {
        false
    }
    /// The module an import specifier names and the identifier it binds in the importing file, when
    /// calls qualified by that identifier resolve into the module (Python `import x as y` -> `("x", "y")`).
    fn import_binding<'a>(&self, _raw_target: &'a str) -> Option<(&'a str, &'a str)> {
        None
    }
    /// The `(target, bound name)` pairs an import statement binds for qualified calls: the target is what
    /// `resolve_import_target` reads. The default is the single pair `import_binding` names, if any.
    fn import_module_bindings(&self, raw_target: &str) -> Vec<(String, String)> {
        self.import_binding(raw_target)
            .map(|(_, bound)| vec![(raw_target.to_string(), bound.to_string())])
            .unwrap_or_default()
    }
    /// True where an import declaration re-exports the names it binds, so a path through a facade module
    /// reaches the file that defines the name (Rust `pub use`, `use super::*`).
    fn import_reexports(&self) -> bool {
        false
    }
    /// False where `name(...)` always calls a function even when a parameter shares the name (Ruby).
    fn parameters_shadow_calls(&self) -> bool {
        true
    }
    /// Project manifests whose module-path aliases apply to this language (e.g. `tsconfig.json`).
    fn import_alias_manifests(&self) -> &'static [&'static str] {
        &[]
    }
    /// `(pattern, target)` aliases a manifest declares, `target` relative to the manifest's directory.
    fn parse_import_aliases(&self, _content: &str) -> Vec<(String, String)> {
        Vec::new()
    }
    /// Resolve an import/include specifier from `importer_path` to a target `file_id` in `file_index`.
    fn resolve_import_target(
        &self,
        importer_path: &str,
        raw_target: &str,
        file_index: &HashMap<&str, u32>,
    ) -> Option<u32> {
        let clean = strip_import_quotes(raw_target);
        if clean.is_empty() {
            return None;
        }
        resolve_relative_or_exact(importer_path, clean, file_index, &[])
    }

    /// Lint rule families this language opts into.
    fn lint_groups(&self) -> &'static [LintGroup] {
        &[]
    }

    /// True if `word` is a keyword of this language (operator, not operand, in readability and clone tokens).
    fn is_keyword(&self, word: &[u8]) -> bool {
        common::keywords::common(word)
    }

    /// Path-sensitive grammar shared by source analyses.
    fn source_grammar(&self, _path: &str) -> Option<&'static tree_sitter::Language> {
        self.grammar_and_query().map(|(language, _)| language)
    }

    /// Catalog rules that also fire on this language although `lang=` does not list it.
    fn lint_extra_rules(&self) -> &'static [&'static str] {
        &[]
    }

    fn lint_name_scope<'a>(&self, _scope: Option<&'a str>) -> Option<&'a str> {
        None
    }

    /// True if `name` is reserved for the implementation (C-family `__x`, `_X`).
    fn is_reserved_identifier(&self, _name: &str) -> bool {
        false
    }

    /// Declared return type in a callable's signature text, when the grammar names one.
    fn lint_return_type<'a>(&self, _sig: &'a str, _name: &str) -> Option<&'a str> {
        None
    }

    /// Spellings of the boolean type a predicate-named callable may return.
    fn lint_bool_types(&self) -> &'static [&'static str] {
        &["bool", "boolean", "Bool", "Boolean", "_Bool"]
    }

    fn lint_void_types(&self) -> &'static [&'static str] {
        &["void", "Void", "None"]
    }

    /// Leading keywords of a declaration that defines a type instead of a variable.
    fn type_declaration_keywords(&self) -> &'static [&'static str] {
        &[]
    }

    /// The expression a self-assignment compares against its target: grammars that flatten
    /// selector chains (`x = x.f()`) parse only the leading identifier as the right-hand value.
    fn assigned_value_root<'a>(&self, rhs: &'a str) -> &'a str {
        rhs
    }

    fn lint_constant_initializer(&self, node: tree_sitter::Node<'_>, source: &str) -> bool {
        lint_hooks::constant_initializer(node, source)
    }

    /// True when the cache-layout lint family applies (template containers, loop bodies).
    fn supports_cache_lint(&self) -> bool {
        false
    }
}
