use crate::support::corpus::fixture;
use ripwire_rs::Lang;
use ripwire_rs::metrics::nonlocal::{NonlocalFunction, find_nonlocal_state};

pub fn state_report(lang: Lang, files: &[(&str, &str)]) -> Vec<NonlocalFunction> {
    let fx = fixture(lang, files);
    find_nonlocal_state(&fx.ingest, &fx.graph, &fx.interner)
}

pub fn function<'a>(report: &'a [NonlocalFunction], name: &str) -> Option<&'a NonlocalFunction> {
    report.iter().find(|f| f.sym_name == name)
}
