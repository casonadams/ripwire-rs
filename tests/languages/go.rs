use super::harness::check_owner_callees;
use crate::support::index::index_files;
use ripwire_rs::Lang;

#[test]
fn test_go_graph_protocol() {
    assert!(Lang::Go.semantics().is_symbol_name_public("Serve"));
    assert!(!Lang::Go.semantics().is_symbol_name_public("handleConn"));
}

#[test]
fn test_go_method_arity_ignores_receiver_list() {
    let source = b"package main\ntype Store struct{}\nfunc (s *Store) Put(key string, v int) {}\nfunc run() {\n\ts := &Store{}\n\ts.Put(\"a\", 1)\n}\n";
    let (ingest, _, interner) = index_files(&[("store.go", Lang::Go, source)]);
    let put = ingest
        .symbols
        .iter()
        .find(|s| interner.resolve(&s.name) == "Put")
        .expect("Put is indexed");
    assert_eq!(
        put.params, 2,
        "the receiver list must not count as a parameter"
    );
}

const UNTYPED: &[u8] = b"package store\n\ntype Context struct{}\n\ntype Store struct{}\n\nfunc (s *Store) Put(key string) {}\n\nfunc run() {\n\tsrc := provider()\n\tsrc.Put(\"a\")\n\tsrc.Context()\n}\n";
const LOCALS: &[u8] = b"package store\n\ntype Store struct{ n int }\n\nfunc (s *Store) Put(key string) {}\n\nfunc NewStore() *Store { return &Store{} }\n\ntype Cache struct{}\n\nfunc (c Cache) Get(key string) {}\n\nfunc fill(s *Store) {\n\ts.Put(\"a\")\n}\n\nfunc run() {\n\ta := NewStore()\n\ta.Put(\"x\")\n\tb := &Store{}\n\tb.Put(\"y\")\n\tvar c Cache\n\tc.Get(\"z\")\n}\n";
const ROUTER: &[u8] = b"package router\n\ntype Mux struct{}\n\nfunc NewMux() *Mux { return &Mux{} }\n\nfunc (m *Mux) Use(x int) {}\n";
const APP: &[u8] = b"package app\n\nimport \"example.com/x/router\"\n\nfunc build() {\n\tm := router.NewMux()\n\tm.Use(1)\n}\n";
const STD_WRITER: &[u8] = b"package store\n\nimport \"net/http\"\n\ntype Writer struct{}\n\nfunc (w *Writer) Write(p []byte) {}\n\nfunc send(w http.ResponseWriter) {\n\tw.Write(nil)\n}\n";
const EMBEDDED: &[u8] = b"package svc\n\ntype Handler struct{}\n\nfunc (h *Handler) Serve() {}\n\ntype Base struct{}\n\nfunc (b *Base) Ping() {}\n\ntype Server struct {\n\tBase\n\thandler *Handler\n}\n\nfunc (s *Server) Run() {\n\ts.handler.Serve()\n\ts.Ping()\n}\n";
const PARAM_NAMED: &[u8] = b"package server\n\ntype Server struct{}\n\nfunc (s *Server) handle() {}\n\nfunc (s *Server) Run(handle int) {\n\ts.handle()\n}\n";

/// One case: the corpus files, the caller to read, and its owner-qualified callees in sorted order.
type Row = (
    &'static [(&'static str, &'static [u8])],
    &'static str,
    &'static [&'static str],
);

#[test]
fn go_typed_receivers_bind_by_declared_type() {
    let rows: &[Row] = &[
        (&[("store.go", UNTYPED)], "run", &["Store.Put"]),
        (&[("store.go", LOCALS)], "fill", &["Store.Put"]),
        (
            &[("store.go", LOCALS)],
            "run",
            &[".NewStore", "Cache.Get", "Store.Put"],
        ),
        (
            &[("router/router.go", ROUTER), ("app/app.go", APP)],
            "build",
            &[".NewMux", "Mux.Use"],
        ),
        (&[("writer.go", STD_WRITER)], "send", &[]),
        (
            &[("svc.go", EMBEDDED)],
            "Run",
            &["Base.Ping", "Handler.Serve"],
        ),
        (&[("server.go", PARAM_NAMED)], "Run", &["Server.handle"]),
    ];
    for (paths, caller, expected) in rows {
        let files: Vec<(&str, Lang, &[u8])> = paths
            .iter()
            .map(|&(path, source)| (path, Lang::Go, source))
            .collect();
        check_owner_callees(&files, caller, expected);
    }
}
