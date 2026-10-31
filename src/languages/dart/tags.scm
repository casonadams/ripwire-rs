; Dart — adapted from tree-sitter-dart's own queries/tags.scm (MIT). The reference patterns
; are rewritten: upstream's trailing @reference.call is an optional-heavy match over selector
; chains that fires on the RECEIVER as well as the invoked member. Verified against the real
; parse tree, not the upstream file; test/dartcheck.sh pins every claim below.

; ── definitions ───────────────────────────────────────────────────────────────────────
(class_definition
  name: (identifier) @name) @definition.class

(mixin_declaration
  (identifier) @name) @definition.class

(extension_declaration
  name: (identifier) @name) @definition.class

(enum_declaration
  (identifier) @name) @definition.class

(type_alias
  (type_identifier) @name) @definition.type

(function_signature
  name: (identifier) @name) @definition.function

(method_signature
  (function_signature
    name: (identifier) @name)) @definition.method

(method_signature
  (getter_signature
    name: (identifier) @name)) @definition.method

(method_signature
  (setter_signature
    name: (identifier) @name)) @definition.method

; A constructor's signature carries the CLASS identifier first, then the constructor name for
; the named form (`C.seeded`). The first identifier is captured for the unnamed form (`C()`); the
; gate in mod.rs drops it for the named form, where the second identifier is the member name
; (`seeded`, owner `C`) and is captured by the second pattern. Factories follow the same shape.
(declaration
  (constructor_signature
    .
    (identifier) @name)) @definition.method

(declaration
  (constructor_signature
    .
    (identifier)
    .
    (identifier) @name)) @definition.method

(method_signature
  (factory_constructor_signature
    .
    (identifier) @name)) @definition.method

(method_signature
  (factory_constructor_signature
    .
    (identifier)
    .
    (identifier) @name)) @definition.method

; ── references ────────────────────────────────────────────────────────────────────────
; `selector` holds exactly ONE child, so a call is a pair of SIBLING selectors, never nested.
; The wildcard parent is deliberate: calls appear under function_body, block, expression_
; statement, return_statement, arguments and a dozen more, and enumerating them would rot.

; `f(args)` — an identifier whose IMMEDIATE next sibling is the argument list.
(_ (identifier) @name
   .
   (selector (argument_part))) @reference.call

; `recv.member(args)` / `recv?.member(args)` — the MEMBER is the call, never the receiver.
; The receiver's own next sibling is the member selector, not an argument_part, so the
; pattern above cannot also fire on it.
(_ (selector
     [ (unconditional_assignable_selector (identifier) @name)
       (conditional_assignable_selector (identifier) @name) ])
   .
   (selector (argument_part))) @reference.call

; `..member(args)` inside a cascade. The cascade receiver is a sibling `this`/identifier node
; outside the cascade_section and is deliberately not captured: `this..add(1)` is one call to
; add, not a call to `this`.
(cascade_section
  (cascade_selector (identifier) @name)
  (argument_part)) @reference.call

(new_expression
  (type_identifier) @name) @reference.class

(superclass
  (type_identifier) @name) @reference.extends

(interfaces
  (type_identifier) @name) @reference.implementation

(mixins
  (type_identifier) @name) @reference.implementation

(uri
  (string_literal) @name) @reference.import

; ── stated floors ─────────────────────────────────────────────────────────────────────
; * Named constructors and factories index as members of the class (`C.seeded()` is member
;   `seeded` with owner `C`). The unnamed `C()` / `factory C()` index under the class name and
;   are contract members (mod.rs): a by-name call `C(...)` binds to the class alone.
;   Calls spelled `C.seeded(1)` bind by owner.
; * Dynamic dispatch through `noSuchMethod` names its callee at run time; not an edge.
; * `part` / `part of` composition is not resolved: a part file's members index under their
;   own file, never the enclosing library.
