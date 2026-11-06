use super::MethodsByFile;
use super::TraitEnv;
use super::TraitImpl;
use super::TraitIndex;
use super::paths::{Want, only, type_leaf};
use crate::domain::{FileInfo, Implementation, Interner, NodeId, SymKind, Symbol};
use crate::resolve::scopes::RustScopes;
use lasso::Spur;
use std::collections::HashMap;

impl TraitIndex {
    #[must_use]
    pub fn build(
        implementations: &[Implementation],
        files: &[FileInfo],
        env: &TraitEnv<'_>,
    ) -> Self {
        let mut index = Self {
            nominals_by_name: nominals_by_name(env.symbols),
            scopes: RustScopes::build(files, env),
            crate_roots: crate_roots(files, env.interner),
            ..Self::default()
        };
        let methods = methods_by_file(env.symbols);
        index.index_trait_bodies(env, &methods);
        for imp in implementations {
            let file = files.get(imp.file_id as usize);
            let inherits = file.is_some_and(|file| file.lang.semantics().has_class_inheritance());
            let declared =
                file.is_some_and(|file| file.lang.semantics().methods_declare_their_receiver());
            index.add_implementation(imp, env, &methods, inherits, declared);
        }
        index.add_structural_implementors(env);
        index
    }

    fn index_trait_bodies(&mut self, env: &TraitEnv<'_>, methods: &MethodsByFile) {
        let traits: Vec<NodeId> = self
            .nominals_by_name
            .values()
            .flatten()
            .copied()
            .filter(|&id| env.symbols[id as usize].kind == SymKind::Interface)
            .collect();
        for trait_id in traits {
            let t = &env.symbols[trait_id as usize];
            let members = span_methods(
                env.symbols,
                file_methods(methods, t.file_id),
                t.sig_start_byte,
                t.end_byte,
            );
            for &m in members {
                self.non_inherent.insert(m);
                self.trait_members.entry(trait_id).or_default().push(m);
                if env.symbols[m as usize].has_body() {
                    self.trait_defaults.entry(trait_id).or_default().push(m);
                }
            }
        }
    }

    /// Classes that satisfy a structurally typed interface by shape: each interface method has a
    /// same-named method with the same parameter count on the class. Such a class is recorded as an
    /// implementor of the interface (dispatch only), never as a subject, so `implements` checks and
    /// unrelated classes stay as declared.
    fn add_structural_implementors(&mut self, env: &TraitEnv<'_>) {
        let symbols = env.symbols;
        let mut interfaces: Vec<NodeId> = self
            .nominals_by_name
            .values()
            .flatten()
            .copied()
            .filter(|&id| {
                let sym = &symbols[id as usize];
                sym.kind == SymKind::Interface && sym.lang.semantics().interfaces_are_structural()
            })
            .collect();
        interfaces.sort_unstable();
        let mut subjects: Vec<NodeId> = self.by_subject.keys().copied().collect();
        subjects.sort_unstable();
        for iface in interfaces {
            let required: Vec<(Spur, u16)> = self
                .trait_members
                .get(&iface)
                .into_iter()
                .flatten()
                .map(|&m| (symbols[m as usize].name, symbols[m as usize].params))
                .collect();
            if required.is_empty() {
                continue;
            }
            for &subject in &subjects {
                let methods: Vec<NodeId> = self.by_subject[&subject]
                    .iter()
                    .flat_map(|&i| self.impls[i].methods.iter().copied())
                    .collect();
                let conforms = required.iter().all(|&(name, params)| {
                    methods.iter().any(|&m| {
                        let sym = &symbols[m as usize];
                        sym.name == name && sym.params == params && sym.has_body()
                    })
                });
                if conforms {
                    let idx = self.impls.len();
                    self.impls.push(TraitImpl {
                        trait_id: Some(iface),
                        subject: Some(subject),
                        bound: None,
                        methods,
                    });
                    self.by_trait.entry(iface).or_default().push(idx);
                }
            }
        }
    }

    fn add_implementation(
        &mut self,
        imp: &Implementation,
        env: &TraitEnv<'_>,
        methods: &MethodsByFile,
        inherits: bool,
        receivers_declared: bool,
    ) {
        let members = span_methods(
            env.symbols,
            file_methods(methods, imp.file_id),
            imp.start_byte,
            imp.end_byte,
        );
        if imp.trait_name.is_some() {
            self.non_inherent.extend(members.iter().copied());
        }
        let trait_id = self.resolve_trait_of(imp, env);
        let (subject, bound) = self.resolve_subject_and_bound(imp, env);
        let subject = subject.or_else(|| {
            (inherits && imp.trait_name.is_some() && !imp.generic_subject)
                .then(|| {
                    self.resolve_text(imp.subject, imp.file_id, imp.start_byte, env, Want::Any)
                })
                .flatten()
        });
        if inherits && (trait_id.is_none() || imp.extends) {
            self.record_supertype(imp, subject, env);
        }
        let external =
            !receivers_declared && !imp.generic_subject && subject.is_none() && !members.is_empty();
        if trait_id.is_none() && subject.is_none() && bound.is_none() && !external {
            return;
        }
        let idx = self.impls.len();
        self.impls.push(TraitImpl {
            trait_id,
            subject,
            bound,
            methods: members.to_vec(),
        });
        if let Some(trait_id) = trait_id {
            self.by_trait.entry(trait_id).or_default().push(idx);
        }
        if imp.generic_subject {
            if bound.is_some() {
                self.blankets.push(idx);
            }
        } else if let Some(subject) = subject {
            self.by_subject.entry(subject).or_default().push(idx);
        } else if let Some(text) = env.interner.try_resolve(&imp.subject) {
            self.by_external_subject
                .entry(type_leaf(text).to_string())
                .or_default()
                .push(idx);
        }
    }

    fn record_supertype(
        &mut self,
        imp: &Implementation,
        subject: Option<NodeId>,
        env: &TraitEnv<'_>,
    ) {
        let (Some(subject), Some(name)) = (subject, imp.trait_name) else {
            return;
        };
        let want = if imp.extends {
            Want::Any
        } else {
            Want::Concrete
        };
        let base = self.resolve_text(name, imp.file_id, imp.start_byte, env, want);
        if let Some(base) = base.filter(|&base| base != subject) {
            let bases = self.supertypes.entry(subject).or_default();
            if !bases.contains(&base) {
                bases.push(base);
            }
        }
    }

    fn resolve_subject_and_bound(
        &self,
        imp: &Implementation,
        env: &TraitEnv<'_>,
    ) -> (Option<NodeId>, Option<NodeId>) {
        let resolve = |text, want| self.resolve_text(text, imp.file_id, imp.start_byte, env, want);
        if imp.generic_subject {
            (
                None,
                imp.subject_bound.and_then(|b| resolve(b, Want::Trait)),
            )
        } else {
            (resolve(imp.subject, Want::Concrete), None)
        }
    }

    fn resolve_trait_of(&self, imp: &Implementation, env: &TraitEnv<'_>) -> Option<NodeId> {
        let name = env.interner.try_resolve(&imp.trait_name?)?;
        let text = match imp.trait_qualifier {
            Some(q) => format!("{}::{name}", env.interner.try_resolve(&q)?),
            None => name.to_owned(),
        };
        self.resolve_path(&text, imp.file_id, imp.start_byte, env, Want::Trait)
    }
}

fn nominals_by_name(symbols: &[Symbol]) -> HashMap<Spur, Vec<NodeId>> {
    let mut map: HashMap<Spur, Vec<NodeId>> = HashMap::new();
    for s in symbols.iter().filter(|s| {
        let semantics = s.lang.semantics();
        semantics.indexes_implementations()
            && (s.kind.is_type_container()
                || (s.kind == SymKind::Other && semantics.named_types_take_methods())
                || (s.kind == SymKind::Section && semantics.modules_are_mixins()))
    }) {
        map.entry(s.name).or_default().push(s.id);
    }
    map
}

fn methods_by_file(symbols: &[Symbol]) -> MethodsByFile {
    let mut map: MethodsByFile = HashMap::new();
    for s in symbols.iter().filter(|s| is_indexed_method(s)) {
        map.entry(s.file_id).or_default().push(s.id);
    }
    for ids in map.values_mut() {
        ids.sort_unstable_by_key(|&id| symbols[id as usize].sig_start_byte);
    }
    map
}

/// A method the trait index lists: a `Method`, or a type-scoped function in a language whose functions
/// are its type's members.
fn is_indexed_method(s: &Symbol) -> bool {
    let semantics = s.lang.semantics();
    semantics.indexes_implementations()
        && (s.kind == SymKind::Method
            || (s.kind == SymKind::Function
                && s.scope.is_some()
                && semantics.functions_are_type_members()))
}

fn file_methods(methods: &MethodsByFile, file_id: u32) -> &[NodeId] {
    methods.get(&file_id).map(Vec::as_slice).unwrap_or(&[])
}

fn span_methods<'a>(
    symbols: &[Symbol],
    methods: &'a [NodeId],
    start: u32,
    end: u32,
) -> &'a [NodeId] {
    let lo = methods.partition_point(|&id| symbols[id as usize].sig_start_byte < start);
    let hi = methods.partition_point(|&id| symbols[id as usize].sig_start_byte < end);
    &methods[lo..hi.max(lo)]
}

/// The crate root file of every file: the unique `lib.rs`/`main.rs` under the deepest `src/`
/// directory containing it. A root file is its own crate root.
fn crate_roots(files: &[FileInfo], interner: &Interner) -> Vec<Option<u32>> {
    let paths: Vec<(u32, &str)> = files
        .iter()
        .filter_map(|f| Some((f.id, interner.try_resolve(&f.path)?)))
        .collect();
    let roots: Vec<(u32, &str)> = files
        .iter()
        .filter_map(|f| {
            let path = interner.try_resolve(&f.path)?;
            let suffixes = f.lang.semantics().crate_root_suffixes();
            suffixes
                .iter()
                .any(|s| path.ends_with(s))
                .then_some((f.id, path))
        })
        .collect();
    let len = files.iter().map(|f| f.id as usize + 1).max().unwrap_or(0);
    let mut out = vec![None; len];
    for &(id, path) in &paths {
        out[id as usize] = crate_root_of(id, path, &roots);
    }
    out
}

fn crate_root_of(id: u32, path: &str, roots: &[(u32, &str)]) -> Option<u32> {
    if roots.iter().any(|&(root, _)| root == id) {
        return Some(id);
    }
    let containing: Vec<(usize, u32)> = roots
        .iter()
        .filter_map(|&(root, root_path)| {
            let dir = &root_path[..=root_path.rfind('/')?];
            path.starts_with(dir).then_some((dir.len(), root))
        })
        .collect();
    let deepest = containing.iter().map(|&(depth, _)| depth).max()?;
    only(
        containing
            .into_iter()
            .filter(|&(depth, _)| depth == deepest)
            .map(|(_, root)| root),
    )
}
