#[path = "support/corpus.rs"]
mod corpus_support;

#[path = "support/a_safety_context.rs"]
mod a_safety_context;

mod query {
    mod body_policy;
    mod commands;
    mod defs;
    mod facts;
    mod module_anchor;
    mod retrieval_rank;
    mod safety;
    mod stray_lines;
}
