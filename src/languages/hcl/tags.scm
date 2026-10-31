; ripwire-rs HCL / Terraform tags
;
; A labelled block is named by its FINAL label: `resource "aws_s3_bucket" "logs"` defines `logs`,
; `module "network"` defines `network`. Before this, the block keyword (`resource`) was the name, so
; every resource in a Terraform tree collided under one symbol.
;
; Every label before the final one is a type-like name: the resource type in `resource "aws_s3_bucket"
; "logs"` is a section, so `--defs=aws_s3_bucket` still finds it.

(block
  (string_lit
    (template_literal) @name) . (string_lit)) @definition.section

(block
  (string_lit
    (template_literal) @name) . (block_start)) @definition.class

; Unlabelled blocks (`locals { ... }`, `terraform { ... }`) are named by their keyword.
(block
  . (identifier) @name . (block_start)) @definition.class

(attribute
  (identifier) @name) @definition.field

(function_call
  (identifier) @name) @reference.call
