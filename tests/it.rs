//! One integration-test binary: each former `tests/<mod>.rs` root is a module here.

#[path = "support/a_callees.rs"]
mod a_callees;
#[path = "support/a_safety_context.rs"]
mod a_safety_context;
#[path = "support/b_ingest.rs"]
mod b_ingest;
#[path = "support/b_state.rs"]
mod b_state;
#[path = "support/corpus.rs"]
mod corpus_support;
#[path = "support/index.rs"]
mod index_support;
#[path = "support/nav.rs"]
mod nav_support;
#[path = "support/nonlocal.rs"]
mod nonlocal_support;
#[path = "support/uses.rs"]
mod uses_support;

mod cache;
mod cli;
mod clones;
mod commands;
mod crawler;
mod domain;
mod gate;
mod git;
mod graph;
mod ingest;
mod languages;
mod mcp;
mod metrics;
mod navigate;
mod query;
mod rank;
mod resolve;
mod serialize;
mod skills;
mod slice;
mod workspace;
