#[path = "support/nav.rs"]
mod nav_support;

mod resolve {
    mod callables;
    mod imported_std_names;
    mod ladder;
    mod rust_imports;
    mod scip;
    mod trait_dispatch;
}
