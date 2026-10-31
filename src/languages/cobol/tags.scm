; ripwire-rs COBOL tags

; ---- definitions ----

(identification_division
  (program_name) @name) @definition.module

(paragraph_header) @name @definition.function

(section_header) @name @definition.function

; ---- calls ----

; `PERFORM A THRU B` names two paragraphs: one reference per label, so both bind (the procedure node
; text is the two names joined, which binds to nothing).
(perform_procedure
  (label
    (qualified_word
      (WORD) @name))) @reference.call

; `GO TO FINISH-UP` transfers control to a paragraph: a call edge like PERFORM.
(goto_statement
  to: (label
    (qualified_word
      (WORD) @name))) @reference.call

(call_statement
  x: _ @name) @reference.call

; ---- imports ----

(copy_statement
  book: _ @import.path)
