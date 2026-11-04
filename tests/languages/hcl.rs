use super::harness::{GraphProtocolCase, sym, verify_protocol_case};
use ripwire_rs::{Lang, SymKind};

/// Two resources of one type are two symbols, named by their own labels, not one `resource` symbol.
#[test]
fn test_hcl_labelled_blocks_are_named_by_their_final_label() {
    verify_protocol_case(&GraphProtocolCase {
        label: "hcl-labels",
        files: &[(
            "infra/main.tf",
            Lang::Hcl,
            b"resource \"aws_s3_bucket\" \"logs\" {\n  bucket = \"a\"\n}\n\nresource \"aws_s3_bucket\" \"audit\" {\n  bucket = \"b\"\n}\n\nmodule \"network\" {\n  source = \"./net\"\n}\n\nlocals {\n  x = 1\n}\n",
        )],
        expected_syms: &[
            sym("logs", SymKind::Class, None, false),
            sym("audit", SymKind::Class, None, false),
            sym("network", SymKind::Class, None, false),
            sym("locals", SymKind::Class, None, false),
            sym("aws_s3_bucket", SymKind::Section, None, false),
        ],
        rejected_syms: &["resource"],
        ..Default::default()
    });
}
