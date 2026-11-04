use super::harness::resolved_edges;
use ripwire_rs::Lang;

#[test]
fn test_sql_statements_reference_the_tables_they_touch() {
    let edges = resolved_edges(&[(
        "schema.sql",
        Lang::Sql,
        b"CREATE TABLE users (\n  id bigint PRIMARY KEY\n);\nCREATE TABLE orders (\n  id bigint PRIMARY KEY,\n  user_id bigint REFERENCES users(id)\n);\nCREATE VIEW active_users AS\n  SELECT u.id FROM users u JOIN orders o ON o.user_id = u.id;\nCREATE FUNCTION touch_users() RETURNS bigint AS $$\n  UPDATE users SET id = id WHERE id = 1;\n$$ LANGUAGE sql;\nCREATE FUNCTION purge_orders() RETURNS bigint AS $$\n  DELETE FROM orders WHERE user_id = 2;\n$$ LANGUAGE sql;\n",
    )]);
    // REFERENCES in a column definition, FROM and JOIN relations, UPDATE target, and the DELETE FROM form.
    for (from, to) in [
        ("orders", "users"),
        ("active_users", "users"),
        ("active_users", "orders"),
        ("touch_users", "users"),
        ("purge_orders", "orders"),
    ] {
        let edge = (from.to_owned(), to.to_owned());
        assert!(edges.contains(&edge), "missing {edge:?} in {edges:?}");
    }
}

#[test]
fn test_sql_extensions_include_dataform_sqlx() {
    assert_eq!(Lang::from_extension("sql"), Lang::Sql);
    assert_eq!(Lang::from_extension("sqlx"), Lang::Sql);
}

fn sqlx_edges(files: &[(&str, &[u8])]) -> (Vec<String>, Vec<(String, String)>) {
    use ripwire_rs::{Interner, LanguageRegistry, ingest_in_memory_files, resolve_all_references};
    let inputs: Vec<(&str, Lang, &[u8])> = files.iter().map(|(p, s)| (*p, Lang::Sql, *s)).collect();
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(&inputs, &registry, &interner);
    let name = |id: u32| {
        interner
            .resolve(&ingest.symbols[id as usize].name)
            .to_owned()
    };
    let mut symbols: Vec<String> = ingest.symbols.iter().map(|s| name(s.id)).collect();
    symbols.sort();
    let mut edges: Vec<(String, String)> = resolve_all_references(&ingest, &interner)
        .edge_accumulators
        .keys()
        .map(|(from, to)| (name(*from), name(*to)))
        .collect();
    edges.sort();
    (symbols, edges)
}

#[test]
fn sqlx_models_define_tables_and_link_ref_lineage() {
    let (symbols, edges) = sqlx_edges(&[
        (
            "definitions/raw_person.sqlx",
            b"config { type: \"declaration\", name: \"raw_person\" }\n",
        ),
        (
            "definitions/person.sqlx",
            b"config {\n  type: \"table\",\n  name: \"person\",\n  description: \"{ not a block }\"\n}\n\nSELECT id FROM ${ref(\"raw_person\")}\n",
        ),
        (
            "definitions/order_totals.sqlx",
            b"config { type: \"view\", dependencies: [\"person\"] }\nSELECT * FROM ${ref(\"schema\", \"raw_person\")}\n",
        ),
    ]);
    assert_eq!(symbols, ["order_totals", "person", "raw_person"]);
    assert_eq!(
        edges,
        [
            ("order_totals".into(), "person".into()),
            ("order_totals".into(), "raw_person".into()),
            ("person".into(), "raw_person".into()),
        ]
    );
}

#[test]
fn sqlx_without_config_is_named_by_file_stem_and_ignores_lookalike_calls() {
    let (symbols, edges) = sqlx_edges(&[(
        "definitions/silver/shopify_order.sqlx",
        b"SELECT 1 FROM ${prefref(\"nope\")} -- plain\n",
    )]);
    assert_eq!(symbols, ["shopify_order"]);
    assert!(edges.is_empty());
}
