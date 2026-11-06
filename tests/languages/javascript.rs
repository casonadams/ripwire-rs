use crate::support::index::index_files;
use ripwire_rs::{Lang, RefRole};

fn indexed_names(files: &[(&str, Lang, &[u8])]) -> Vec<String> {
    let (ingest, _, interner) = index_files(files);
    ingest
        .symbols
        .iter()
        .map(|s| interner.resolve(&s.name).to_owned())
        .collect()
}

#[test]
fn test_javascript_member_assigned_function_is_named_by_its_property() {
    // `helper` is what callers write (`mod.helper()`); the inner name `helperFn` is not a second definition.
    let names = indexed_names(&[(
        "lib/store.js",
        Lang::JavaScript,
        b"module.exports.helper = function helperFn(x) { return x * 2; };\nStore.prototype.reset = function resetStore() {};\nconst other = {};\nother.inner = function innerFn() {};\n",
    )]);
    assert!(names.contains(&"helper".to_owned()));
    assert!(names.contains(&"reset".to_owned()));
    assert!(!names.contains(&"helperFn".to_owned()));
    assert!(!names.contains(&"resetStore".to_owned()));
    // An assignment to an ordinary object is not an export or a prototype member: its inner name stays.
    assert!(names.contains(&"innerFn".to_owned()));
}

#[test]
fn test_javascript_and_jsx_graph_protocol() {
    let (ingest, _, interner) = index_files(&[(
        "src/View.jsx",
        Lang::JavaScript,
        b"module.exports.exportedFn = () => {};\nother.notExport = () => {};\nFoo.prototype.protoFn = function() {};\nexport function View() { return <span><Footer /></span>; }",
    )]);
    let syms: Vec<&str> = ingest
        .symbols
        .iter()
        .map(|s| interner.resolve(&s.name))
        .collect();
    assert!(syms.contains(&"exportedFn"));
    assert!(syms.contains(&"protoFn"));
    assert!(!syms.contains(&"notExport"));
    let calls: Vec<&str> = ingest
        .references
        .iter()
        .filter(|r| r.role == RefRole::Call)
        .map(|r| interner.resolve(&r.callee_name))
        .collect();
    assert!(calls.contains(&"Footer"));
    assert!(!calls.contains(&"span"));
}

const JS_QUEUE: &str = "class DLL {\n  push(item) { return item; }\n  shift() { return 1; }\n}\n\
class Heap {\n  push(item) { return item; }\n  toArray() { return []; }\n}\n\
function makeHeap() { return new Heap(); }\n\
class Queue {\n  constructor() { this.tasks = new DLL(); }\n\
  add(item) { this.tasks.push(item); }\n\
  viaLocal() { const heap = new Heap(); heap.push(1); }\n\
  viaObject() { const q = { _tasks: new DLL() }; q._tasks.shift(); }\n\
  viaShapeName() { const q = { _tasks: new DLL() }; q.push(1); }\n\
  viaResult() { return makeHeap().toArray(); }\n\
  viaArgument(list) { list.push(1); }\n\
}\n\
function Pool() {}\n\
Pool.prototype.fill = function () { this.tasks = new DLL(); };\n\
Pool.prototype.drain = function () { this.tasks.shift(); };\n";

#[test]
fn javascript_typed_receivers_bind_their_declared_owner_method() {
    let files: [(&str, Lang, &[u8]); 1] = [("lib/queue.js", Lang::JavaScript, JS_QUEUE.as_bytes())];
    let cases: &[(&str, &[&str])] = &[
        ("add", &["DLL.push"]),
        ("viaLocal", &[".Heap", "Heap.push"]),
        ("viaObject", &[".DLL", "DLL.shift"]),
        ("viaShapeName", &[".DLL"]),
        ("viaResult", &[".makeHeap", "Heap.toArray"]),
        ("viaArgument", &[]),
        ("drain", &["DLL.shift"]),
    ];
    for &(caller, want) in cases {
        let got = super::harness::owner_qualified_callees(&files, caller);
        let got: Vec<&str> = got.iter().map(String::as_str).collect();
        assert_eq!(got, want, "{caller}");
    }
}
