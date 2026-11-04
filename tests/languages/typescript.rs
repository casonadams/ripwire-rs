use ripwire_rs::{Interner, Lang, LanguageRegistry, RefRole, ingest_in_memory_files};
use std::collections::HashMap;

#[test]
fn test_typescript_and_tsx_graph_protocol() {
    let reg = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[(
            "src/App.tsx",
            Lang::TypeScript,
            b"export function App() { return <div><Header /><UI.Button>ok</UI.Button></div>; }",
        )],
        &reg,
        &interner,
    );
    let calls: Vec<&str> = ingest
        .references
        .iter()
        .filter(|r| r.role == RefRole::Call)
        .map(|r| interner.resolve(&r.callee_name))
        .collect();
    assert!(calls.contains(&"Header"));
    assert!(calls.contains(&"Button"));
    assert!(!calls.contains(&"div"));

    let mut file_index = HashMap::new();
    file_index.insert("packages/utils/src/index.ts", 1);
    file_index.insert("packages/utils/src/logger.ts", 2);
    file_index.insert("packages/agent/src/helper.ts", 3);
    let sem = Lang::TypeScript.semantics();
    assert_eq!(
        sem.resolve_import_target(
            "packages/agent/src/main.ts",
            "'@scope/pkg-utils'",
            &file_index
        ),
        Some(1)
    );
    assert_eq!(
        sem.resolve_import_target(
            "packages/agent/src/main.ts",
            "'@scope/utils/logger'",
            &file_index
        ),
        Some(2)
    );
    assert_eq!(
        sem.resolve_import_target("packages/agent/src/main.ts", "'./helper.js'", &file_index),
        Some(3)
    );
    assert_eq!(
        sem.resolve_import_target("packages/agent/src/main.ts", "'external-pkg'", &file_index),
        None
    );
}
