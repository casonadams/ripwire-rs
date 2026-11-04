use super::harness::owner_qualified_callees;
use ripwire_rs::Lang;

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
