use super::harness::{GraphProtocolCase, owner_qualified_callees, sym, verify_protocol_case};
use ripwire_rs::{Lang, SymKind};
use std::collections::HashMap;

#[test]
fn test_python_graph_protocol_and_gates() {
    verify_protocol_case(&GraphProtocolCase {
        label: "python",
        files: &[(
            "pkg/worker.py",
            Lang::Python,
            b"import os\nclass Mode(Enum):\n    FAST = 1\nclass BaseWorker:\n    pass\nclass TaskWorker(BaseWorker):\n    untyped_noise = 0\n    def execute(self, other):\n        self.job_id = 7\n        other.temp_val = 20\n        os.getcwd()\n        run_step()\ndef run_step():\n    return 1\nself.top_level = 30\n",
        )],
        expected_syms: &[
            sym("FAST", SymKind::Var, Some("Mode"), false),
            sym("TaskWorker", SymKind::Class, None, true),
            sym("execute", SymKind::Function, Some("TaskWorker"), true),
            sym("job_id", SymKind::Field, Some("TaskWorker"), false),
            sym("run_step", SymKind::Function, None, true),
        ],
        rejected_syms: &["untyped_noise", "temp_val", "top_level"],
        expected_edges: &[("execute", "run_step")],
        expect_external: true,
        expected_lego: Some(("BaseWorker", "TaskWorker")),
        expected_import: Some("os"),
    });

    let mut file_index = HashMap::new();
    file_index.insert("pkg/mod_a.py", 6);
    let sem = Lang::Python.semantics();
    assert_eq!(
        sem.resolve_import_target("pkg/sub.py", ".mod_a", &file_index),
        Some(6)
    );
    assert!(!sem.is_entrypoint_symbol("main", None, SymKind::Function, "docs/build_docs.py"));
}

#[test]
fn python_member_calls_reach_class_members_not_other_modules_functions() {
    let helpers =
        b"def parse_args():\n    return 1\n\ndef parse(text):\n    return text\n".as_slice();
    let app = b"import argparse\nimport ast\nfrom store import Store\n\ndef main(store: Store):\n    parser = argparse.ArgumentParser()\n    parser.parse_args()\n    ast.parse('x')\n    store.save()\n".as_slice();
    let store = b"class Store:\n    def save(self):\n        return 1\n".as_slice();
    let callees = owner_qualified_callees(
        &[
            ("scripts/helpers.py", Lang::Python, helpers),
            ("scripts/app.py", Lang::Python, app),
            ("scripts/store.py", Lang::Python, store),
        ],
        "main",
    );
    assert_eq!(callees, ["Store.save"]);
}

/// A bare `len(raw)` is the builtin unless the module binds `len`; another module's `len` is no callee.
#[test]
fn python_bare_builtin_calls_do_not_bind_to_other_modules() {
    let util =
        b"def len(x):\n    return 0\n\ndef make_record(raw):\n    return {'raw': raw}\n".as_slice();
    let store = b"from util import make_record\n\ndef build(raw):\n    n = len(raw)\n    return make_record(n)\n".as_slice();
    let callees = |caller| {
        owner_qualified_callees(
            &[
                ("pkg/util.py", Lang::Python, util),
                ("pkg/store.py", Lang::Python, store),
            ],
            caller,
        )
    };
    assert_eq!(callees("build"), [".make_record"]);
}

/// A module that defines a builtin's name itself keeps its own calls to that name.
#[test]
fn python_module_defined_builtin_name_still_binds_locally() {
    let files =
        b"def open(path):\n    return path\n\ndef load(path):\n    return open(path)\n".as_slice();
    let callees =
        |caller| owner_qualified_callees(&[("pkg/files.py", Lang::Python, files)], caller);
    assert_eq!(callees("load"), [".open"]);
}

/// `@dec` is a call of `dec` from the file scope; `@property` names the builtin and edges to nothing.
#[test]
fn python_bare_decorators_are_calls_of_their_names() {
    let module = b"def dec(f):\n    return f\n\nclass Shape:\n    @property\n    def area(self):\n        return 1\n\n@dec\ndef decorated():\n    return 1\n".as_slice();
    let callees =
        owner_qualified_callees(&[("pkg/shapes.py", Lang::Python, module)], "<file-scope>");
    assert_eq!(callees, [".dec"]);
    let area = owner_qualified_callees(&[("pkg/shapes.py", Lang::Python, module)], "area");
    assert!(area.is_empty());
}

/// A receiver annotated or constructed as a class binds that class's method, not another's.
#[test]
fn python_typed_receivers_bind_their_declared_class() {
    let module = b"class Cache:\n    def fetch_one(self, k):\n        return k\n\nclass Queue:\n    def fetch_one(self, k):\n        return k\n\ndef use(c: Cache):\n    return c.fetch_one(3)\n\ndef build():\n    cache = Cache()\n    return cache.fetch_one(1)\n\ndef mixed(c: Cache):\n    c = Queue()\n    return c.fetch_one(2)\n".as_slice();
    let callees =
        |caller| owner_qualified_callees(&[("pkg/cache.py", Lang::Python, module)], caller);
    assert_eq!(callees("use"), ["Cache.fetch_one"]);
    assert_eq!(callees("build"), [".Cache", "Cache.fetch_one"]);
    assert_eq!(
        callees("mixed"),
        [".Queue", "Cache.fetch_one", "Queue.fetch_one"]
    );
}
