; ripwire-rs SQL tags
(create_table
  (object_reference
    name: (identifier) @name)) @definition.class

(create_view
  (object_reference
    name: (identifier) @name)) @definition.class

(create_function
  (object_reference
    name: (identifier) @name)) @definition.function

; ---- table references in a .sql file's own statements ----
; Shapes from the vendored DerekStride grammar (e038788e), verified with an AST dump: the FROM and JOIN
; relations sit under `from` (select) and `join`; a DELETE's `from` is a sibling of `delete` under the
; statement; an UPDATE target is a `relation` under `update`; INSERT's target and a REFERENCES target are
; direct object_reference children. A schema-qualified `s.t` keeps `t` as its `name:` field.
(from
  (relation
    (object_reference
      name: (identifier) @name))) @reference.read

(join
  (relation
    (object_reference
      name: (identifier) @name))) @reference.read

(statement
  (delete)
  (from
    (object_reference
      name: (identifier) @name))) @reference.write

(update
  (relation
    (object_reference
      name: (identifier) @name))) @reference.write

(insert
  (object_reference
    name: (identifier) @name)) @reference.write

(column_definition
  (keyword_references)
  (object_reference
    name: (identifier) @name)) @reference.read

(create_index
  (object_reference
    name: (identifier) @name)) @reference.read
