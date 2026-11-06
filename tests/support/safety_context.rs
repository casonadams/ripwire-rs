use crate::support::corpus::fixture;
use ripwire_rs::Lang;
use ripwire_rs::gate::contract::SignatureContract;
use ripwire_rs::query::safety::{EditCheckRequest, SafetyContext, write_edit_check_from_facts};

/// Indexes one in-memory Rust corpus and hands `body` the safety context built over it.
pub fn with_safety_context<R>(
    root: &str,
    files: &[(&str, &str)],
    body: impl FnOnce(&SafetyContext<'_>) -> R,
) -> R {
    let corpus = fixture(Lang::Rust, files);
    body(&SafetyContext {
        root,
        ingest: &corpus.ingest,
        graph: &corpus.graph,
        interner: &corpus.interner,
    })
}

/// The edit-check report for `target`, whose head contract takes one parameter.
pub fn target_edit_check_xml(
    context: &SafetyContext<'_>,
    page: Option<(usize, usize)>,
    at_stamp: &str,
) -> String {
    let baseline = SignatureContract {
        name: "target".into(),
        params: 1,
        required_params: 1,
        is_public: true,
    };
    let mut out = Vec::new();
    write_edit_check_from_facts(
        &EditCheckRequest {
            sym_name: "target",
            head_contract: Some(&baseline),
            page,
            at_stamp: Some(at_stamp),
        },
        context,
        &mut out,
    )
    .unwrap();
    String::from_utf8(out).unwrap()
}
