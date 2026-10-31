; ripwire-rs Solidity tags
(function_definition
    name: (identifier) @name) @definition.function

(contract_declaration
    name: (identifier) @name) @definition.class

(interface_declaration
    name: (identifier) @name) @definition.interface

(library_declaration
    name: (identifier) @name) @definition.interface

(struct_declaration
    name: (identifier) @name) @definition.class

(enum_declaration
    name: (identifier) @name) @definition.class

(call_expression
    (expression (identifier) @name)) @reference.call

(call_expression
    (expression (member_expression
        property: (identifier) @name))) @reference.call

(inheritance_specifier
    ancestor: (user_defined_type
        (identifier) @name)) @reference.extends

; `new Vault(...)` constructs the contract `Vault`: a call on the named type.
(new_expression
    name: (type_name
        (user_defined_type
            (identifier) @name))) @reference.call
