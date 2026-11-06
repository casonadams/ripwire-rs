use super::harness::{nested_owner_callees, owner_qualified_callees};
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

type Callees = fn(&[(&str, Lang, &[u8])], &str) -> Vec<String>;

/// Asserts the callees of each `(caller, expected)` case, read with `callees` (owner-qualified or file-labelled).
fn assert_callees(files: &[(&str, Lang, &[u8])], cases: &[(&str, &[&str])], callees: Callees) {
    for &(caller, want) in cases {
        let got = callees(files, caller);
        let got: Vec<&str> = got.iter().map(String::as_str).collect();
        assert_eq!(got, want, "{caller}");
    }
}

const QUEUE_CASES: &[(&str, &[&str])] = &[
    ("add", &["DLL.push"]),
    ("viaLocal", &[".Heap", "Heap.push"]),
    ("viaObject", &[".DLL", "DLL.shift"]),
    ("viaShapeName", &[".DLL"]),
    ("viaResult", &[".makeHeap", "Heap.toArray"]),
    ("viaArgument", &[]),
    ("drain", &["DLL.shift"]),
];

#[test]
fn javascript_typed_receivers_bind_their_declared_owner_method() {
    let files = [("lib/queue.js", Lang::JavaScript, JS_QUEUE.as_bytes())];
    assert_callees(&files, QUEUE_CASES, owner_qualified_callees);
}

/// A tree with bundled copies: `Heap`, `Cache` and `isAsync` are each declared in `lib/` and again in `dist/`.
/// Callers reach the `lib/` declaration only through what they import.
const IMPORT_TREE: [(&str, &str); 8] = [
    (
        "lib/internal/Heap.js",
        "export default class Heap {\n  push(item) { return item; }\n  shift() { return 1; }\n}\n",
    ),
    (
        "lib/internal/DoublyLinkedList.js",
        "export default class DLL {\n  push(item) { return item; }\n}\n",
    ),
    (
        "lib/internal/wrapAsync.js",
        "export function isAsync(fn) { return fn.constructor.name === 'AsyncFunction'; }\nexport default function wrapAsync(fn) { return fn; }\n",
    ),
    (
        "lib/cache.js",
        "export default class Cache {\n  get(key) { return key; }\n}\n",
    ),
    (
        "dist/async.js",
        "class Heap {\n  push(item) { return item; }\n  shift() { return 1; }\n}\nclass Cache {\n  get(key) { return key; }\n}\nfunction isAsync(fn) { return fn; }\n",
    ),
    (
        "test/require.js",
        "var Heap = require('../lib/internal/Heap').default;\n\
var {isAsync} = require('../lib/internal/wrapAsync');\n\
const {default: wrapAsync} = require('../lib/internal/wrapAsync');\n\
function viaRequire() { var heap = new Heap(); heap.push(1); heap.shift(); }\n\
function viaAsync(fn) { return isAsync(fn); }\n\
function viaWrap(fn) { return wrapAsync(fn); }\n",
    ),
    (
        "test/import.js",
        "import Cache from '../lib/cache.js';\nimport DLL from '../lib/internal/DoublyLinkedList';\n\
function viaImport() { const cache = new Cache(); return cache.get('k'); }\n\
function viaDll() { const dll = new DLL(); return dll.push(1); }\n",
    ),
    (
        "test/builtin.js",
        "const Cache = require('../lib/cache').default;\n\
function viaBuiltin() { const m = new Map(); return m.get('k'); }\n\
function viaSet() { const s = new Set(); return s.has('k'); }\n",
    ),
];

fn import_tree() -> Vec<(&'static str, Lang, &'static [u8])> {
    IMPORT_TREE
        .iter()
        .map(|(path, src)| (*path, Lang::JavaScript, src.as_bytes()))
        .collect()
}

const IMPORT_CASES: &[(&str, &[&str])] = &[
    (
        "viaRequire",
        &[
            "lib/internal/Heap.js:.Heap",
            "lib/internal/Heap.js:Heap.push",
            "lib/internal/Heap.js:Heap.shift",
        ],
    ),
    ("viaAsync", &["lib/internal/wrapAsync.js:.isAsync"]),
    ("viaWrap", &["lib/internal/wrapAsync.js:.wrapAsync"]),
    (
        "viaImport",
        &["lib/cache.js:.Cache", "lib/cache.js:Cache.get"],
    ),
    (
        "viaDll",
        &[
            "lib/internal/DoublyLinkedList.js:.DLL",
            "lib/internal/DoublyLinkedList.js:DLL.push",
        ],
    ),
    ("viaBuiltin", &[]),
    ("viaSet", &[]),
];

#[test]
fn javascript_require_and_import_bind_names_to_the_declaring_file() {
    assert_callees(&import_tree(), IMPORT_CASES, nested_owner_callees);
}
