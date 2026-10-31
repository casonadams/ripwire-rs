use ripwire_rs::slice::list_sliceable_locals;
use ripwire_rs::{Lang, LanguageRegistry, OccRole};

#[test]
fn test_occ_role_properties() {
    let roles = [
        (OccRole::Param, true, false, "param"),
        (OccRole::Decl, true, false, "decl"),
        (OccRole::Assign, true, true, "assign"),
        (OccRole::CallArg, false, true, "call-arg"),
        (OccRole::Read, false, true, "read"),
    ];
    for (r, def, is_use, tag) in roles {
        assert_eq!(r.is_def(), def);
        assert_eq!(r.is_use(), is_use);
        assert_eq!(r.as_str(), tag);
    }
}

#[test]
fn test_slice_locals_ts_rust_cpp_destructured_and_typed() {
    let reg = LanguageRegistry::new().unwrap();
    let mut parser = tree_sitter::Parser::new();

    let (ts_lang, _) = reg.get(Lang::TypeScript).unwrap();
    parser.set_language(ts_lang).unwrap();
    let ts_src = b"export function buildModel<TApi extends string>(spec: TApi): Model<TApi> {\n  const policy = resolve(spec);\n  return policy;\n}\n";
    let ts_tree = parser.parse(ts_src, None).unwrap();
    let ts_locals = list_sliceable_locals(
        ts_tree.root_node(),
        ts_src,
        Lang::TypeScript.semantics().syntax_kinds(),
    );
    let ts_names: Vec<&str> = ts_locals.iter().map(|(n, _, _)| n.as_str()).collect();
    assert_eq!(ts_names, vec!["spec", "policy"]);

    let (rs_lang, _) = reg.get(Lang::Rust).unwrap();
    parser.set_language(rs_lang).unwrap();
    let rs_src = b"fn render(input: &str) {\n  let (vis_w, vis_q) = split(input);\n  let mut lines = init(&vis_w, &vis_q);\n  lines.push(1);\n}\n";
    let rs_tree = parser.parse(rs_src, None).unwrap();
    let rs_locals = list_sliceable_locals(
        rs_tree.root_node(),
        rs_src,
        Lang::Rust.semantics().syntax_kinds(),
    );
    let rs_names: Vec<&str> = rs_locals.iter().map(|(n, _, _)| n.as_str()).collect();
    assert_eq!(rs_names, vec!["input", "vis_w", "vis_q", "lines"]);

    let (cpp_lang, _) = reg.get(Lang::Cpp).unwrap();
    parser.set_language(cpp_lang).unwrap();
    let cpp_src = b"Communities louvain(const Graph& adj) {\n  Communities out;\n  out.comm.assign(10, 0);\n  std::vector<int> comm(10);\n  return out;\n}\n";
    let cpp_tree = parser.parse(cpp_src, None).unwrap();
    let cpp_locals = list_sliceable_locals(
        cpp_tree.root_node(),
        cpp_src,
        Lang::Cpp.semantics().syntax_kinds(),
    );
    let cpp_names: Vec<&str> = cpp_locals.iter().map(|(n, _, _)| n.as_str()).collect();
    assert_eq!(cpp_names, vec!["adj", "out", "comm"]);
}

#[test]
fn rust_for_loop_variable_is_a_sliceable_local() {
    let reg = LanguageRegistry::new().unwrap();
    let (rs_lang, _) = reg.get(Lang::Rust).unwrap();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(rs_lang).unwrap();
    let src = b"fn total(xs: &[i32]) -> i32 {\n  let mut sum = 0;\n  for x in xs {\n    sum += x;\n  }\n  sum\n}\n";
    let tree = parser.parse(src, None).unwrap();
    let locals =
        list_sliceable_locals(tree.root_node(), src, Lang::Rust.semantics().syntax_kinds());
    let names: Vec<&str> = locals.iter().map(|(n, _, _)| n.as_str()).collect();
    assert_eq!(names, vec!["xs", "sum", "x"]);
}
