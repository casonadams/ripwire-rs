#[path = "support/a_callees.rs"]
mod a_callees;

#[path = "support/nav.rs"]
mod nav_support;

mod resolve {
    mod callables;
    mod imported_std_names;
    mod ladder;
    mod rust_imports;
    mod rust_reexports;
    mod scip;
    mod trait_dispatch;
}
