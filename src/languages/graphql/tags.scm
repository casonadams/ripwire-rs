; ripwire-rs GraphQL tags. Shapes from the tree-sitter-graphql 0.3.0 grammar node types.

; ---- definitions ----
(object_type_definition
  (name) @name) @definition.class

(interface_type_definition
  (name) @name) @definition.interface

; enum, input, union and scalar types are named types a field or argument refers to, so they are classes too
(enum_type_definition
  (name) @name) @definition.class

(input_object_type_definition
  (name) @name) @definition.class

(union_type_definition
  (name) @name) @definition.class

(scalar_type_definition
  (name) @name) @definition.class

(operation_definition
  (name) @name) @definition.function

; a named fragment is invoked by each `...Name` spread, so it is a callable definition
(fragment_definition
  (fragment_name
    (name) @name)) @definition.function

(field_definition
  (name) @name) @definition.field

; ---- references ----
; `...Name` invokes the fragment it names
(fragment_spread
  (fragment_name
    (name) @name)) @reference.call

; every named type mention: field and argument types, implemented interfaces, union members, fragment and
; inline-fragment type conditions, variable types, schema root types
(named_type
  (name) @name) @reference.type
