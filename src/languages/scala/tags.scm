; ripwire-rs Scala tags
(template_body
  (function_definition
    name: (identifier) @name) @definition.method)

(template_body
  (function_declaration
    name: (identifier) @name) @definition.method)

(compilation_unit
  (function_definition
    name: (identifier) @name) @definition.function)

(block
  (function_definition
    name: (identifier) @name) @definition.function)

(indented_block
  (function_definition
    name: (identifier) @name) @definition.function)

(class_definition
  name: (identifier) @name) @definition.class

(trait_definition
  name: (identifier) @name) @definition.interface

(object_definition
  name: (identifier) @name) @definition.class

; Scala 3 `enum Color { case Red, Green }` — the enum is a type container (Java's enum uses the
; same @definition.type bucket). Without this, the enum and its methods were orphaned.
(enum_definition
  name: (identifier) @name) @definition.type

; `new Box(..)` / `new Box[T](..)` / `new pkg.Box(..)` — an instance creation resolves to the
; constructed class, the same shape as Java's object_creation_expression. Without these the
; constructor edge was missing (only the calls were captured).
(instance_expression
  (type_identifier) @name) @reference.call

(instance_expression
  (applied_constructor_type
    (type_identifier) @name)) @reference.call

(instance_expression
  (generic_type
    (type_identifier) @name)) @reference.call

(instance_expression
  (stable_type_identifier
    (type_identifier) @name)) @reference.call

(call_expression
  function: (identifier) @name) @reference.call


(call_expression
  function: (field_expression
    field: (identifier) @name)) @reference.call
(extends_clause
  type: (type_identifier) @name) @reference.extends
