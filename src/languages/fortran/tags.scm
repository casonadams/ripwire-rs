; ripwire-rs Fortran tags

; ---- definitions ----

; A named generic interface (`interface scale` with `module procedure scale_real`) is a procedure
; family: `scale(a, 2.0)` calls the generic, so the generic name is defined here.
(interface
  (interface_statement
    (name) @name)) @definition.interface

(derived_type_statement
  (type_name) @name) @definition.class

(program
  (program_statement
    (name) @name)) @definition.module

(module
  (module_statement
    (name) @name)) @definition.module

(submodule_statement
  (module_name) (name) @name) @definition.module

(interface
  (function
    (function_statement
      (name) @name) @definition.interface))

(interface
  (subroutine
    (subroutine_statement
      (name) @name) @definition.interface))

(function
  (function_statement
    (name) @name)) @definition.function

(subroutine
  (subroutine_statement
    (name) @name)) @definition.function

; ---- calls ----

(call_expression
  (identifier) @name) @reference.call

(subroutine_call
  (identifier) @name) @reference.call

; ---- imports ----

(use_statement
  (module_name) @import.path)
