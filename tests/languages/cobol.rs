use super::harness::{GraphProtocolCase, owner_qualified_callees, sym, verify_protocol_case};
use ripwire_rs::{Lang, SymKind};

#[test]
fn test_cobol_graph_protocol() {
    let code = b"       IDENTIFICATION DIVISION.\n       PROGRAM-ID. PAYROLL.\n       PROCEDURE DIVISION.\n       MAIN-LOGIC.\n           PERFORM CALC-TAX.\n           STOP RUN.\n       CALC-TAX.\n           DISPLAY \"CALCULATING\".\n";
    verify_protocol_case(&GraphProtocolCase {
        label: "cobol",
        files: &[("src/payroll.cbl", Lang::Cobol, code)],
        expected_syms: &[
            sym("MAIN-LOGIC", SymKind::Function, None, false),
            sym("CALC-TAX", SymKind::Function, None, false),
        ],
        expected_edges: &[("MAIN-LOGIC", "CALC-TAX")],
        ..Default::default()
    });
    assert!(
        Lang::Cobol
            .semantics()
            .is_std_or_builtin_member("current-date")
    );
    assert!(
        Lang::Cobol
            .semantics()
            .is_std_or_builtin_qualifier("sysout")
    );
    assert_eq!(Lang::from_extension("cbl"), Lang::Cobol);
    assert_eq!(Lang::from_extension("cob"), Lang::Cobol);
    assert_eq!(Lang::from_extension("cpy"), Lang::Cobol);
}

/// `PERFORM A THRU B` reaches both paragraphs, and `GO TO C` is a transfer edge like PERFORM.
#[test]
fn cobol_perform_thru_and_go_to_are_edges_to_each_paragraph() {
    let source = b"       IDENTIFICATION DIVISION.\n       PROGRAM-ID. LEDGER.\n       PROCEDURE DIVISION.\n       MAIN-LINE.\n           PERFORM OPEN-FILES THRU CLOSE-FILES.\n           GO TO FINISH-UP.\n       OPEN-FILES.\n           DISPLAY 'OPEN'.\n       CLOSE-FILES.\n           DISPLAY 'CLOSE'.\n       FINISH-UP.\n           STOP RUN.\n";
    let files: &[(&str, Lang, &[u8])] = &[("src/ledger.cbl", Lang::Cobol, source)];
    assert_eq!(
        owner_qualified_callees(files, "MAIN-LINE"),
        vec![
            ".CLOSE-FILES".to_string(),
            ".FINISH-UP".to_string(),
            ".OPEN-FILES".to_string(),
        ]
    );
}
