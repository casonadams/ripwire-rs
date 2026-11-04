mod build;
mod dispatch;
mod paths;

use super::scopes::RustScopes;
use crate::domain::{Interner, NodeId, Symbol};
use lasso::Spur;
use std::collections::{HashMap, HashSet};

/// Symbol, directory, interner, and module-qualifier lookups that owner resolution reads.
pub struct TraitEnv<'a> {
    pub symbols: &'a [Symbol],
    pub file_dirs: &'a [u32],
    pub interner: &'a Interner,
    pub qualifier_file: &'a dyn Fn(u32, Spur) -> Option<u32>,
}

/// One impl block. `trait_id` is set only when the trait is an indexed local trait; `subject`
/// is the concrete type the block implements for; `bound` is the trait a blanket impl requires.
#[derive(Debug, Clone)]
pub struct TraitImpl {
    pub trait_id: Option<NodeId>,
    pub subject: Option<NodeId>,
    pub bound: Option<NodeId>,
    pub methods: Vec<NodeId>,
}

#[derive(Debug, Default, Clone)]
pub struct TraitIndex {
    impls: Vec<TraitImpl>,
    by_trait: HashMap<NodeId, Vec<usize>>,
    by_subject: HashMap<NodeId, Vec<usize>>,
    by_external_subject: HashMap<String, Vec<usize>>,
    blankets: Vec<usize>,
    trait_defaults: HashMap<NodeId, Vec<NodeId>>,
    trait_members: HashMap<NodeId, Vec<NodeId>>,
    nominals_by_name: HashMap<Spur, Vec<NodeId>>,
    scopes: RustScopes,
    crate_roots: Vec<Option<u32>>,
    non_inherent: HashSet<NodeId>,
    supertypes: HashMap<NodeId, Vec<NodeId>>,
}

type MethodsByFile = HashMap<u32, Vec<NodeId>>;
