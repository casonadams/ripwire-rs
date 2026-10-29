use ripwire_rs::serialize::PageRequest;
use ripwire_rs::{
    CloneIdiom, CloneType, Interner, Lang, LanguageRegistry, find_clones, ingest_in_memory_files,
    render_clones_xml,
};
use std::collections::HashMap;

#[test]
fn test_clone_type1_and_type2_detection() {
    let interner = Interner::new();
    let registry = LanguageRegistry::new().unwrap();

    let code1 = b"fn func_one(a: i32, b: i32) -> i32 {\n    let sum = a + b;\n    let prod = a * b;\n    return sum + prod + 100;\n}\n";
    let code2 = b"fn func_two(a: i32, b: i32) -> i32 {\n    let sum = a + b;\n    let prod = a * b;\n    return sum + prod + 100;\n}\n";
    let code3 = b"fn func_three(x: i32, y: i32) -> i32 {\n    let alpha = x + y;\n    let beta = x * y;\n    return alpha + beta + 100;\n}\n";

    let mut sources = HashMap::new();
    sources.insert("src/a.rs".into(), code1.to_vec());
    sources.insert("src/b.rs".into(), code2.to_vec());
    sources.insert("src/c.rs".into(), code3.to_vec());

    let ingest = ingest_in_memory_files(
        &[
            ("src/a.rs", Lang::Rust, code1),
            ("src/b.rs", Lang::Rust, code2),
            ("src/c.rs", Lang::Rust, code3),
        ],
        &registry,
        &interner,
    );

    let clones = find_clones(&ingest, &interner, 10, Some(&sources));
    assert!(!clones.is_empty(), "Should detect clone group");
    assert_eq!(clones[0].members.len(), 3);
    assert_eq!(clones[0].clone_type, CloneType::Type2);
    let xml = render_clones_xml(&clones, &ingest, &interner, PageRequest::default());
    assert!(xml.contains("<clones schema=\"ripwire-rs.clones/v1\""));
    assert!(xml.contains("<f n=\"func_one\""));
    assert!(xml.contains("<group type=\"2\""));
    assert_eq!(CloneIdiom::None.as_str(), None);
    assert_eq!(
        CloneIdiom::ThresholdLadder.as_str(),
        Some("threshold-ladder")
    );
    assert_eq!(
        CloneIdiom::SwitchNameTable.as_str(),
        Some("switch-name-table")
    );
    assert_eq!(CloneIdiom::BuilderChain.as_str(), Some("builder-chain"));
}

#[test]
fn test_clone_type3_gapped_detection() {
    let interner = Interner::new();
    let registry = LanguageRegistry::new().unwrap();

    let code_base = b"fn process_items(items: &[i32]) -> i32 {\n    let mut total = 0;\n    for &item in items {\n        if item > 0 {\n            total += item * 2;\n        }\n    }\n    total\n}\n";
    let code_near = b"fn process_values(values: &[i32]) -> i32 {\n    let mut total = 0;\n    let bonus = 1;\n    for &elem in values {\n        if elem > 0 {\n            total += elem * 2;\n        }\n    }\n    total + bonus\n}\n";

    let mut sources = HashMap::new();
    sources.insert("src/base.rs".into(), code_base.to_vec());
    sources.insert("src/near.rs".into(), code_near.to_vec());

    let ingest = ingest_in_memory_files(
        &[
            ("src/base.rs", Lang::Rust, code_base),
            ("src/near.rs", Lang::Rust, code_near),
        ],
        &registry,
        &interner,
    );

    let clones = find_clones(&ingest, &interner, 10, Some(&sources));
    assert!(!clones.is_empty(), "Should detect Type-3 near-miss clone");
    let t3 = clones.iter().find(|g| g.clone_type == CloneType::Type3);
    assert!(t3.is_some(), "Expected Type-3 clone");
    let g = t3.unwrap();
    assert_eq!(g.members.len(), 2);
    assert!(g.similarity >= 0.80 && g.similarity < 1.0);
}

#[test]
fn short_unrelated_chains_do_not_form_a_type3_group() {
    let interner = Interner::new();
    let registry = LanguageRegistry::new().unwrap();
    let directory = b"fn compute_directory(path: &str) -> String {\n    Path::new(path)\n        .parent()\n        .and_then(|p| p.to_str())\n        .filter(|s| !s.is_empty())\n        .unwrap_or(\".\")\n        .to_string()\n}\n";
    let names = b"fn names(xml: &str) -> BTreeSet<String> {\n    roxmltree::Document::parse(xml)\n        .unwrap()\n        .descendants()\n        .filter_map(|node| node.attribute(\"n\"))\n        .map(str::to_owned)\n        .collect()\n}\n";
    let mut sources = HashMap::new();
    sources.insert("src/gate/seams.rs".into(), directory.to_vec());
    sources.insert("tests/cli/dispatch_contract.rs".into(), names.to_vec());
    let ingest = ingest_in_memory_files(
        &[
            ("src/gate/seams.rs", Lang::Rust, directory),
            ("tests/cli/dispatch_contract.rs", Lang::Rust, names),
        ],
        &registry,
        &interner,
    );
    let clones = find_clones(&ingest, &interner, 10, Some(&sources));
    assert!(
        clones.iter().all(|g| g.clone_type != CloneType::Type3),
        "unrelated chains reached the Type-3 floor: {:?}",
        clones.iter().map(|g| g.similarity).collect::<Vec<_>>()
    );
}

#[test]
fn clone_report_honors_limit_and_offset() {
    let interner = Interner::new();
    let registry = LanguageRegistry::new().unwrap();
    let sum = |name: &str| {
        format!(
            "fn {name}(a: i32, b: i32) -> i32 {{\n    let sum = a + b;\n    let prod = a * b;\n    return sum + prod + 100;\n}}\n"
        )
    };
    let sign = |name: &str| {
        format!(
            "fn {name}(v: i32) -> i32 {{\n    if v > 0 {{\n        return v * 2;\n    }}\n    return v - 1;\n}}\n"
        )
    };
    let bodies = [sum("one"), sum("two"), sign("pos"), sign("neg")];
    let paths = ["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs"];
    let files: Vec<_> = paths
        .iter()
        .zip(&bodies)
        .map(|(p, b)| (*p, Lang::Rust, b.as_bytes()))
        .collect();
    let sources: HashMap<String, Vec<u8>> = paths
        .iter()
        .zip(&bodies)
        .map(|(p, b)| ((*p).into(), b.as_bytes().to_vec()))
        .collect();
    let ingest = ingest_in_memory_files(&files, &registry, &interner);
    let clones = find_clones(&ingest, &interner, 10, Some(&sources));
    assert_eq!(clones.len(), 2);
    let report = |limit, offset, budget| {
        let page = PageRequest {
            limit,
            offset,
            budget,
        };
        let xml = render_clones_xml(&clones, &ingest, &interner, page);
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let root = doc.root_element();
        let groups = doc
            .descendants()
            .filter(|n| n.has_tag_name("group"))
            .count();
        (
            groups,
            root.attribute("shown").map(str::to_string),
            root.attribute("has_more").map(str::to_string),
        )
    };
    assert_eq!(report(None, None, None).0, 2);
    assert_eq!(report(None, None, Some(usize::MAX)).0, 2);
    assert_eq!(
        report(None, None, Some(1)),
        (1, Some("1".into()), Some("1".into()))
    );
    assert_eq!(
        report(Some(1), None, Some(usize::MAX)),
        (1, Some("1".into()), Some("1".into()))
    );
    assert_eq!(
        report(Some(1), Some(1), None),
        (1, Some("1".into()), Some("0".into()))
    );
}

#[test]
fn test_clones_touching_a_file_keep_only_groups_with_a_member_there() {
    let interner = Interner::new();
    let registry = LanguageRegistry::new().unwrap();
    let near = |name: &str, last: &str| {
        format!(
            "fn {name}(a: i32, b: i32) -> i32 {{\n    let sum = a + b;\n    let prod = a * b;\n    let diff = a - b;\n    let quot = a / b;\n    return sum + prod + diff + quot + {last};\n}}\n"
        )
    };
    let other = |name: &str| {
        format!(
            "fn {name}(items: &[u8]) -> usize {{\n    let mut count = 0;\n    for item in items {{\n        if *item > 10 {{\n            count += 1;\n        }}\n    }}\n    count\n}}\n"
        )
    };
    let files = [
        ("src/changed.rs", near("edited", "1")),
        ("src/peer.rs", near("peer", "2")),
        ("src/c.rs", other("count_big")),
        ("src/d.rs", other("count_large")),
    ];
    let sources: HashMap<String, Vec<u8>> = files
        .iter()
        .map(|(p, s)| ((*p).to_string(), s.as_bytes().to_vec()))
        .collect();
    let slices: Vec<_> = files
        .iter()
        .map(|(p, s)| (*p, Lang::Rust, s.as_bytes()))
        .collect();
    let ingest = ingest_in_memory_files(&slices, &registry, &interner);
    let focus: std::collections::HashSet<u32> = [0].into_iter().collect();
    let groups =
        ripwire_rs::find_clones_touching(&ingest, &interner, 10, Some(&sources), None, &focus);
    let names: Vec<Vec<&str>> = groups
        .iter()
        .map(|g| {
            g.members
                .iter()
                .map(|&m| interner.resolve(&ingest.symbols[m as usize].name))
                .collect()
        })
        .collect();
    assert_eq!(names, vec![vec!["edited", "peer"]]);
}
