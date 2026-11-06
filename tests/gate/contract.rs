use crate::support::syntax::parse_rust;
use ripwire_rs::gate::contract::{
    ContractStatus, SignatureContract, check_node_visibility, compare_contracts,
    extract_param_arity, parse_head_signature_match, parse_symbol_contract_from_bytes,
};
use ripwire_rs::{Lang, LanguageRegistry};
use streaming_iterator::StreamingIterator;

#[test]
fn test_compare_contracts_scenarios() {
    let baseline = SignatureContract {
        name: "fetch".to_string(),
        params: 2,
        required_params: 2,
        is_public: false,
    };

    let same = SignatureContract {
        name: "fetch".to_string(),
        params: 2,
        required_params: 2,
        is_public: false,
    };
    assert_eq!(
        compare_contracts(&same, Some(&baseline)),
        ContractStatus::Unchanged
    );

    let arity_broken = SignatureContract {
        name: "fetch".to_string(),
        params: 3,
        required_params: 3,
        is_public: false,
    };
    assert_eq!(
        compare_contracts(&arity_broken, Some(&baseline)),
        ContractStatus::ContractChange {
            param_changed: true,
            public_changed: false,
            was_params: 2,
            now_params: 3,
            was_public: false,
            now_public: false,
        }
    );

    let pub_changed = SignatureContract {
        name: "fetch".to_string(),
        params: 2,
        required_params: 2,
        is_public: true,
    };
    assert_eq!(
        compare_contracts(&pub_changed, Some(&baseline)),
        ContractStatus::ContractChange {
            param_changed: false,
            public_changed: true,
            was_params: 2,
            now_params: 2,
            was_public: false,
            now_public: true,
        }
    );

    assert_eq!(compare_contracts(&same, None), ContractStatus::NewSymbol);
}

fn parse_rust_fn_stats(code: &[u8]) -> (u16, bool) {
    let tree = parse_rust(code);
    let fn_node = tree.root_node().child(0).unwrap();
    (
        extract_param_arity(fn_node, Lang::Rust.semantics().syntax_kinds()).max,
        check_node_visibility(fn_node, code, Lang::Rust),
    )
}

#[test]
fn test_extract_param_and_visibility() {
    assert_eq!(
        parse_rust_fn_stats(b"pub fn example(a: i32, b: i32) {}"),
        (2, true)
    );
}

#[test]
fn test_extract_param_count_ignores_generic_type_parameters() {
    let code = b"pub fn collect_with_rank_and_limit<E, H>(&self, rank: Option<u32>, limit: Option<usize>, heartbeat: H) {}";
    assert_eq!(parse_rust_fn_stats(code).0, 4);
}

#[test]
fn test_java_varargs_count_as_variadic_so_longer_calls_still_bind() {
    let reg = LanguageRegistry::new().unwrap();
    let interner = ripwire_rs::Interner::new();
    let fixtures: &[(&str, Lang, &[u8])] = &[
        (
            "Calc.java",
            Lang::Java,
            b"class Calc {\n  static int sum(int... values) { return 0; }\n}\n",
        ),
        (
            "Use.java",
            Lang::Java,
            b"class Use {\n  int go() { return Calc.sum(1, 2, 3); }\n}\n",
        ),
    ];
    let ingest = ripwire_rs::ingest_in_memory_files(fixtures, &reg, &interner);
    let sum = ingest
        .symbols
        .iter()
        .find(|s| interner.resolve(&s.name) == "sum")
        .unwrap();
    assert_eq!(sum.params, 64);
    let graph = ripwire_rs::Graph::build(&ingest, &interner);
    assert_eq!(graph.callers(sum.id).count(), 1);
}

#[test]
fn test_dart_private_method_visibility_and_named_params() {
    let dart_code = br#"
class SwapRecord {
  SwapRecord(this.a, this.b, {required this.c, this.d, this.e});
  Future<void> run() async {
    await _prepare("first", y: 2);
  }
  Future<void> _prepare(String x, {int y = 1, bool z = false}) async {}
}
void makeSwap() {
  SwapRecord(1, 2, c: 3, d: 4);
}
"#;
    let reg = LanguageRegistry::new().unwrap();
    let (ts_lang, query) = reg.get(Lang::Dart).unwrap();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(ts_lang).unwrap();
    let tree = parser.parse(dart_code, None).unwrap();
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), dart_code.as_slice());

    let mut found_prepare = false;
    let mut found_ctor = false;
    while let Some(m) = matches.next() {
        let Some((name, params, is_pub)) =
            parse_head_signature_match(m, query, dart_code, Lang::Dart)
        else {
            continue;
        };
        if name == "_prepare" {
            found_prepare = true;
            assert!(!is_pub, "private Dart method _prepare must not be public");
            assert_eq!(params.max, 3, "1 positional + 2 named parameters");
            assert_eq!(
                params.required, 1,
                "the positional parameter alone is required"
            );
        } else if name == "SwapRecord" && params.max > 0 {
            found_ctor = true;
            assert!(is_pub);
            assert_eq!(params.max, 5, "2 positional + 3 named parameters");
        }
    }
    assert!(found_prepare);
    assert!(found_ctor);
}

#[test]
fn test_parse_symbol_contract_from_bytes_multi_language() {
    // Rust fixture
    let rust_code = b"pub fn greet(first: &str, last: &str) -> String { format!(\"{first} {last}\") }\nfn secret(code: u32) -> bool { code == 42 }\n";
    let pub_contract = parse_symbol_contract_from_bytes(rust_code, "greet", Lang::Rust).unwrap();
    assert_eq!(
        pub_contract,
        Some(SignatureContract {
            name: "greet".to_string(),
            params: 2,
            required_params: 2,
            is_public: true,
        })
    );

    let priv_contract = parse_symbol_contract_from_bytes(rust_code, "secret", Lang::Rust).unwrap();
    assert_eq!(
        priv_contract,
        Some(SignatureContract {
            name: "secret".to_string(),
            params: 1,
            required_params: 1,
            is_public: false,
        })
    );

    let missing = parse_symbol_contract_from_bytes(rust_code, "not_present", Lang::Rust).unwrap();
    assert_eq!(missing, None);

    // Python fixture
    let py_code = b"def calculate_total(subtotal, tax, discount=0):\n    return subtotal + tax - discount\n\ndef _internal_helper(val):\n    pass\n";
    let py_pub =
        parse_symbol_contract_from_bytes(py_code, "calculate_total", Lang::Python).unwrap();
    assert_eq!(
        py_pub,
        Some(SignatureContract {
            name: "calculate_total".to_string(),
            params: 3,
            required_params: 2,
            is_public: true,
        })
    );

    let py_priv =
        parse_symbol_contract_from_bytes(py_code, "_internal_helper", Lang::Python).unwrap();
    assert_eq!(
        py_priv,
        Some(SignatureContract {
            name: "_internal_helper".to_string(),
            params: 1,
            required_params: 1,
            is_public: false,
        })
    );

    // Go fixture
    let go_code = b"package math\n\nfunc ExportedAdd(a int, b int, c int) int {\n    return a + b + c\n}\n\nfunc unexportedHelper(x int) bool {\n    return x > 0\n}\n";
    let go_pub = parse_symbol_contract_from_bytes(go_code, "ExportedAdd", Lang::Go).unwrap();
    assert_eq!(
        go_pub,
        Some(SignatureContract {
            name: "ExportedAdd".to_string(),
            params: 3,
            required_params: 3,
            is_public: true,
        })
    );

    let go_priv = parse_symbol_contract_from_bytes(go_code, "unexportedHelper", Lang::Go).unwrap();
    assert_eq!(
        go_priv,
        Some(SignatureContract {
            name: "unexportedHelper".to_string(),
            params: 1,
            required_params: 1,
            is_public: false,
        })
    );
}
