#[path = "support/index.rs"]
mod index_support;

mod cli {
    mod architecture_behavior;
    mod args;
    mod dead_code_test_only;
    mod dispatch;
    mod dispatch_contract;
    mod doctor;
    mod error_surfacing;
    mod for_window;
    mod gate_select;
    mod queries;
    mod retrieval_behavior;
    mod safety;
}
