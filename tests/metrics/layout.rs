use ripwire_rs::ToXmlString;
use ripwire_rs::languages::c::layout::parse_struct_fields;
use ripwire_rs::metrics::layout::{
    FieldAffinityReport, FieldLayout, StructLayoutDef, StructLayoutReport,
};
use ripwire_rs::serialize::PageRequest;

#[test]
fn test_layout_parsing() {
    let body = "struct Foo {\n  uint64_t a;\n  uint32_t b;\n  uint8_t c;\n};";
    let fields = parse_struct_fields(body);
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[0].offset, 0);
    assert_eq!(fields[1].offset, 8);
    assert_eq!(fields[2].offset, 12);
}

fn foo_layout(sym: &str) -> StructLayoutReport {
    let def = StructLayoutDef {
        path: "src/test.rs".into(),
        line: 10,
        agg_kind: "struct",
        fields: vec![
            FieldLayout {
                name: "a".into(),
                ty: "u64".into(),
                size: 8,
                align: 8,
                offset: 0,
            },
            FieldLayout {
                name: "b".into(),
                ty: "u32".into(),
                size: 4,
                align: 4,
                offset: 8,
            },
        ],
    };
    StructLayoutReport {
        sym: sym.into(),
        defs: vec![def],
    }
}

#[test]
fn test_field_affinity_lists_offsets_only_for_a_named_struct() {
    let named = FieldAffinityReport {
        layout: foo_layout("Foo"),
        page: PageRequest::default(),
    }
    .to_xml_string();
    assert!(named.contains("schema=\"ripwire-rs.field-affinity/v1\""));
    assert!(named.contains("<f n=\"b\" ty=\"u32\" sz=\"4\" al=\"4\" off=\"8\"/>"));

    let summary = FieldAffinityReport {
        layout: foo_layout(""),
        page: PageRequest::default(),
    }
    .to_xml_string();
    assert!(!summary.contains("<f n="));
}

#[test]
fn test_field_affinity_marks_unmodelled_structs() {
    let unmodelled = StructLayoutDef {
        path: "src/test.rs".into(),
        line: 3,
        agg_kind: "struct",
        fields: Vec::new(),
    };
    let report = FieldAffinityReport {
        layout: StructLayoutReport {
            sym: "Bar".into(),
            defs: vec![unmodelled],
        },
        page: PageRequest::default(),
    }
    .to_xml_string();
    assert!(report.contains("modeled=\"0\""));
    assert!(!report.contains("modeled=\"1\""));
    assert!(!report.contains(" size=\""));

    let modelled = FieldAffinityReport {
        layout: foo_layout("Foo"),
        page: PageRequest::default(),
    }
    .to_xml_string();
    assert!(modelled.contains("modeled=\"1\" fields=\"2\""));
    assert!(modelled.contains(" size=\"16\" align=\"8\" lines=\"1\""));
}
