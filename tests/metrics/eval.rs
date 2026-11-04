use ripwire_rs::metrics::{run_cochange_eval, run_retrieval_eval, run_retrieval_eval_for_repo};
use ripwire_rs::{FileInfo, IngestResult, Interner, Lang, SymKind, Symbol};

#[test]
fn test_eval_output() {
    let mut buf = Vec::new();
    run_cochange_eval(&mut buf).unwrap();
    assert!(
        String::from_utf8(buf)
            .unwrap()
            .contains("ripwire-rs --eval=cochange")
    );

    let mut buf2 = Vec::new();
    run_retrieval_eval(&mut buf2).unwrap();
    assert!(
        String::from_utf8(buf2)
            .unwrap()
            .contains("ripwire-rs --eval=retrieval")
    );

    let interner = Interner::new();
    let mut ingest = IngestResult::new();
    ingest.files.push(FileInfo {
        id: 0,
        path: interner.intern("src/engine.rs"),
        lang: Lang::Rust,
        line_count: 50,
        byte_size: 500,
    });
    ingest.symbols.push(Symbol::new(
        0,
        interner.intern("execute_worker_task"),
        interner.intern("src/engine.rs::execute_worker_task"),
        SymKind::Function,
        Lang::Rust,
        0,
        1,
    ));
    let mut buf3 = Vec::new();
    run_retrieval_eval_for_repo(&ingest, &interner, &[1.0], &mut buf3).unwrap();
    let out3 = String::from_utf8(buf3).unwrap();
    assert!(out3.contains("1 symbols; sample=1"));
    assert!(out3.contains("100.0%"));
}
