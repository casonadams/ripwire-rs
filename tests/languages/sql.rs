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
    let inputs: Vec<(&str, Lang, &[u8])> = files
        .iter()
        .map(|(p, s)| (*p, Lang::from_path(std::path::Path::new(p)), *s))
        .collect();
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

const TABLE: &[u8] = b"config { type: \"table\" }\nSELECT 1\n";

fn consumer_model(tables: &[&str], consumer: &[u8]) -> (Vec<String>, Vec<(String, String)>) {
    let paths: Vec<String> = tables
        .iter()
        .map(|t| format!("definitions/{t}.sqlx"))
        .collect();
    let mut files: Vec<(&str, &[u8])> = paths.iter().map(|p| (p.as_str(), TABLE)).collect();
    files.push(("definitions/consumer.sqlx", consumer));
    sqlx_edges(&files)
}

fn consumer_reads(targets: &[&str]) -> Vec<(String, String)> {
    targets
        .iter()
        .map(|t| ("consumer".into(), (*t).into()))
        .collect()
}

#[test]
fn sqlx_reads_tables_behind_an_interpolated_database_prefix() {
    let (_, edges) = consumer_model(
        &["ref_kind"],
        b"config { type: \"assertion\" }\nSELECT * FROM a\nLEFT JOIN ${dataform.projectConfig.defaultDatabase}.gold.ref_kind k ON TRUE\nWHERE ${other}.x IS NULL\n",
    );
    assert_eq!(edges, consumer_reads(&["ref_kind"]));
}

#[test]
fn sqlx_reads_resolve_calls_and_literal_from_join_paths() {
    let (_, edges) = consumer_model(
        &["ref_kind", "ref_stage", "ref_plan"],
        b"config { type: \"view\" }\nWITH cte AS (SELECT 1)\nSELECT * FROM `proj-1.gold.ref_kind`\njoin gold.ref_stage s ON TRUE\nLEFT JOIN ${resolve(\"ref_plan\")} p ON TRUE\nLEFT JOIN cte ON TRUE\n",
    );
    assert_eq!(
        edges,
        consumer_reads(&["ref_kind", "ref_plan", "ref_stage"])
    );
}

#[test]
fn sqlx_ignores_commented_reads_dynamic_refs_and_self_paths() {
    let (symbols, edges) = consumer_model(
        &["a_", "old", "col"],
        b"// config { name: \"wrong\" }\nconfig { type: \"view\", columns: { name: \"doc\" }, name: \"real\" }\n-- FROM gold.old\n/* ${ref(\"old\")} */\nSELECT ${ref(\"a_\" + suffix)}, ${self()}.col FROM x\n",
    );
    assert_eq!(symbols, ["a_", "col", "old", "real"]);
    assert_eq!(edges, consumer_reads(&[]));
}

#[test]
fn sqlx_resolves_project_vars_in_model_names_and_refs() {
    let (symbols, edges) = sqlx_edges(&[
        (
            "workflow_settings.yaml",
            b"defaultProject: p\nvars:\n  # note\n  log_table: \"billing_log\" # trailing\n  nested:\n    inner: x\nlater: y\n",
        ),
        (
            "definitions/log.sqlx",
            b"config { type: \"table\", name: dataform.projectConfig.vars.log_table }\nSELECT 1\n",
        ),
        (
            "definitions/job.sqlx",
            b"config { type: \"table\" }\nSELECT * FROM ${ref(dataform.projectConfig.vars.log_table)} JOIN ${ref(dataform.projectConfig.vars.unset)}\n",
        ),
        (
            "definitions/orphan.sqlx",
            b"config { name: dataform.projectConfig.vars.absent }\nSELECT 1\n",
        ),
    ]);
    for name in ["billing_log", "job", "orphan"] {
        assert!(symbols.iter().any(|s| s == name), "{name} in {symbols:?}");
    }
    assert!(!symbols.iter().any(|s| s == "log" || s == "absent"));
    assert_eq!(edges, [("job".into(), "billing_log".into())]);
}

#[test]
fn sqlx_project_vars_apply_only_below_their_settings_file() {
    let (symbols, _) = sqlx_edges(&[
        ("proj/dataform.json", br#"{"vars": {"t": "scoped"}}"#),
        (
            "proj/definitions/a.sqlx",
            b"config { name: dataform.projectConfig.vars.t }\nSELECT 1\n",
        ),
        (
            "other/definitions/b.sqlx",
            b"config { name: dataform.projectConfig.vars.t }\nSELECT 1\n",
        ),
    ]);
    assert!(symbols.iter().any(|s| s == "scoped"));
    assert!(symbols.iter().any(|s| s == "b"));
    assert!(!symbols.iter().any(|s| s == "a"));
}
