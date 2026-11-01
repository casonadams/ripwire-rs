; ripwire-rs Zig tags
(function_declaration
  name: (identifier) @name) @definition.function

(variable_declaration
  (identifier) @name
  (struct_declaration)) @definition.class

(variable_declaration
  (identifier) @name
  (enum_declaration)) @definition.type

(variable_declaration
  (identifier) @name
  (union_declaration)) @definition.class

(call_expression
  (identifier) @name) @reference.call

(call_expression
  function: (field_expression
    member: (identifier) @name)) @reference.call

; `const util = @import("util.zig");` — the whole declaration is the import node, so the bound name travels
; with the path and `util.f()` can bind into that file (languages/zig/mod.rs parses the text). Any other
; builtin initializer (`const n = @sizeOf(T);`) matches too and parses to no import.
(variable_declaration
  (identifier)
  [
    (builtin_function)
    (expression (builtin_function))
  ]) @import.path
