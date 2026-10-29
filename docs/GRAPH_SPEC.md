# Graph Protocol Specification

This document specifies the normative 5-layer data graph contract that `ripwire-rs` enforces across all 36 supported Tree-sitter languages (`enum Lang`). Every language grammar (`src/languages/<lang>/tags.scm`) and semantics implementation (`LanguageSemantics` in `src/languages/<lang>/mod.rs`) must satisfy all five layers below.

---

## 1. Layer 1: Vertex Extraction (`IngestResult::symbols`)

Each extracted definition produces a `Symbol` vertex in `IngestResult::symbols` addressed by a dense 32-bit `NodeId`.

### 1.1 Capture Vocabulary Invariant
- `src/languages/<lang>/tags.scm` must use only the closed capture set parsed by `classify_capture` (`src/ingest/extract.rs`):
  - `@name`: Primary symbol identifier token.
  - `@definition.<kind>`: Symbol definition role (`function`, `cjsexport`, `method`, `protomethod`, `class`, `object`, `specialization`, `struct`, `interface`, `constant`, `enummember`, `type`, `var`, `attribute`, `property`, `macro`, `testmacroblock`, `yamlkey`, `module`, `section`, `field`).
  - `@reference.<role>`: Symbol reference role (`call`, `bare`, `operator`, `read`, `attribute`, `write`, `import`, `extends`, `implementation`, `class`, `macro`, `type`, `value`, `through`, `access`). An `access` reference is a member read that may run an accessor (a Python `@property` or `@cached_property`): it binds only to an accessor of that name, and needs receiver evidence (the accessor's own class, or a file naming it) when the name is a standard attribute or more than one accessor shares it; reads that name no accessor are dropped at ingest. TypeScript and JavaScript getters are not modeled: their property reads mostly target interface fields, and binding them by name measured worse against scip-typescript.
  - `@import.path`: Module/header specifier token for file dependency resolution.
- Language-prefixed capture tags are forbidden.

### 1.2 Signature and Body Span Invariant
For every bodied callable (`SymKind::Function | SymKind::Method`):
$$\text{sig\_start\_byte} < \text{sig\_end\_byte} < \text{end\_byte}$$
1. **Signature slice (`sig_start_byte..sig_end_byte`)**: Covers the declaration header up to the start of the body block (`find_def_body_start`), enabling zero-parse signature extraction (`Symbol::extract_signature`).
2. **Full definition span (`sig_start_byte..end_byte`)**: Covers the entire callable body (`LanguageSemantics::resolve_def_body_and_end`), guaranteeing that `find_enclosing_symbol` (`src/ingest/pipeline.rs`) attributes every reference inside the body to the innermost callable `NodeId`.

### 1.3 Enclosing Scope Invariant
- Methods, trait/interface members, and scoped declarations must record their immediate enclosing container in `Symbol::scope` (`Some(Spur)`), yielding canonical identifiers of the form `path::scope::name`.
- Supported container scopes include:
  - Rust `impl_item`, `trait_item`, `mod_item`
  - Go `method_declaration` receiver types (`func (s *Service) Run()` $\to$ `scope = Some("Service")`)
  - OOP classes, structs, interfaces, traits, protocols, objects, modules, and namespaces across C++, ObjC, Java, Kotlin, Scala, C#, Swift, PHP, Python, Ruby, TypeScript, JavaScript, Dart, GDScript, Elixir, and Solidity.

### 1.4 Noise-Free Gate Invariant
- **Convention-gated constants (`sub == Some("constant")`)**: In TypeScript, JavaScript, Java, C#, and Ruby, only `is_screaming_snake` identifiers are indexed; camelCase fields and local bindings are rejected. In C and C++, bindings require `is_screaming_snake`, a `const`/`constexpr`/`constinit` qualifier, or CUDA `__constant__`.
- **YAML mapping depth (`sub == Some("yamlkey")`)**: Mapping depth must be $\le 2$ (with sequence nodes transparent), and `<<` merge keys are rejected.
- **Elixir macro definitions**: Call-shaped definitions require a definition macro target (`def`, `defp`, `defmacro`, `defmacrop`, `defguard`, `defguardp`, `defdelegate`, `defmodule`, `defprotocol`, `defimpl`, `test`) and reject documentation/spec attributes (`@doc`, `@moduledoc`, `@typedoc`, `@spec`, `@impl`, `@behaviour`, `@derive`).
- **C-family empty macros**: Function-like `#define NOOP(x)` guards with no replacement body are rejected.

---

## 2. Layer 2: Enclosing Reference Attribution (`IngestResult::references`)

Every `@reference.*` or `@import.path` match produces a `Reference` record:
- **Innermost Callable Attribution**: `find_enclosing_symbol` assigns `Reference::from_symbol` to the smallest-span `Symbol` whose `[sig_start_byte, end_byte]` encloses `Reference::start_byte`. Every call inside a function or method body must have `from_symbol == callable_id` (never `NO_NODE`, and never the outer class when a method span encloses the call).
- **Receiver Qualifier Extraction**: Qualified calls (`self.helper()`, `this.run()`, `lists:map()`) populate `Reference::qualifier`. Self-receivers (`self`, `this`, `Self`) resolve to the enclosing type scope.
- **Builtin Call Filtering**: Language keywords, C++ casts (`static_cast`, `sizeof`), Rust builtin formatting/assertion macros (`println!`, `assert_eq!`, `vec!`), Dart print/JSON builtins, Elixir special forms, and lowercase JSX intrinsic HTML tags (`<div>`, `<span>`) are rejected at extraction via `LanguageSemantics::should_ignore_call`.

---

## 3. Layer 3: Call Resolution Ladder & Triage Conservation (`Graph` + `ResolutionResult`)

`resolve_all_references` (`src/resolve/`) processes every `RefRole::Call` and `RefRole::Macro` reference and partitions them into four mutually exclusive outcomes (`RefOutcome` / `ResolutionResult`):

$$\text{Total Call Refs} = \text{Resolved} + \text{External} + \text{Declined} + \text{Unresolved}$$

Only calls, macros, and SQL table read/write lineage enter the call CSR.
Ordinary variable reads/writes remain indexed evidence for `--uses`, grep read
locations, and `--nonlocal-state`; target resolution does not turn them into calls.

1. **`RefOutcome::Resolved`**:
   - Bound via the deterministic precedence ladder within the same `DialectGroup`:
     1. Explicit qualifier matching `Symbol::scope`, else the module file an import binds that qualifier to (`LanguageSemantics::import_binding`, e.g. Python `import x as y` for `y.f()`)
     2. Same-file symbol match
     3. Imported-file symbol match (`resolve_import_target`, then the `paths` aliases of the nearest manifest a language declares through `LanguageSemantics::import_alias_manifests`, e.g. `tsconfig.json`/`jsconfig.json`)
     4. Same-directory symbol match (bounded by starburst fan-out cap)
     5. Unique global symbol match across compatible dialects
     6. Polymorphic interface/trait contract fallback (`RefRole::Extends` implementors)
   - Self-loops (`from_symbol == to_symbol`) are discarded when building the CSR `Graph`.
   - An unqualified call or read whose name is a parameter of an enclosing function or closure is local and never enters the ladder; `LanguageSemantics::parameters_shadow_calls` is false for Ruby, where `name(...)` always calls the method.
   - A member call `x.f()` on a receiver the index does not type (`Reference::member_call`) never binds to a non-callable `Var` named `f` when `LanguageSemantics::member_call_targets_callables_only` holds (JavaScript and TypeScript). A property call cannot reach a module or local variable; the call stays unresolved and disclosed.
   - A fluent builder `export const f = createServerFn({..}).handler(async (x) => ..)` takes the parameter count of its final callback (`LanguageSemantics::resolve_def_params_and_name`), so calls to `f` pass the arity check.
2. **`RefOutcome::External`**:
   - Unresolved call whose qualifier matches `LanguageSemantics::is_std_or_builtin_qualifier` (`std`, `fmt`, `os`, `Array`, `Promise`, `System`, `Console`, `Enum`, `NSString`, etc.) or whose unpinned member name matches `LanguageSemantics::is_std_or_builtin_member`.
3. **`RefOutcome::Declined`**:
   - Ambiguous multi-candidate global call that cannot be uniquely bound without type inference.
4. **`RefOutcome::Builtin`**:
   - A rejected standard-member candidate remains in the declined gauge but never appears as an unbound candidate call in `--impact`'s `<dc>` sample.
5. **`RefOutcome::Unresolved`**:
   - Call to a symbol name with zero in-tree definitions and no stdlib qualifier/member match.

---

## 4. Layer 4: Type Hierarchy & Polymorphic Dispatch (`RefRole::Extends` / `--lego`)

- Class inheritance, interface implementation, Rust `impl Trait for Type`, Go/Swift protocol conformance, Ruby `< Super`, PHP `extends`/`implements`, Scala `extends`, Solidity `is`, and GDScript `extends` emit `RefRole::Extends` with `from_symbol` (or enclosing scope) naming the implementing type and `callee_name` naming the base/interface type.
- `call_lego` (`src/mcp/retrieval.rs`) joins interface/trait contract methods (`SymKind::Method` / `SymKind::Function` scoped to the interface) with all `RefRole::Extends` implementors.

---

## 5. Layer 5: File Dependency DAG (`RefRole::Import` / `--deps` & `--arch`)

- Languages with `LanguageSemantics::is_dependency_capable() == true` capture module/header specifiers via `@import.path` or `@reference.import` (`RefRole::Import`).
- ` RefRole::Import` references never enter the symbol call graph (`Graph`); instead, `LanguageSemantics::resolve_import_target` maps `(importer_path, raw_target)` against `file_index` to construct the file-to-file dependency DAG used by `--deps` (Lakos `CCD`/`ACD`/`NCCD`, Tarjan SCC cycles) and `--arch` layer enforcement.
