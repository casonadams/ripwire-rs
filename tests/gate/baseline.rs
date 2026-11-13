use ripwire_rs::gate::baseline::{
    QualitySnapshot, parse_snapshot_from_str, serialize_snapshot_to_string,
};

#[test]
fn test_snapshot_roundtrip_serialization() {
    let mut snap = QualitySnapshot {
        head_sha: "abcdef1234567890abcdef1234567890abcdef12".to_string(),
        dirty: true,
        absorbed: 2,
        ..Default::default()
    };
    snap.ccx_by_sym.insert("src/calc.rs::add".to_string(), 3);
    snap.cx_by_sym.insert("src/calc.rs::add".to_string(), 2);
    snap.loc_by_sym.insert("src/calc.rs::add".to_string(), 10);
    snap.nest_by_sym.insert("src/calc.rs::add".to_string(), 1);
    snap.params_by_sym.insert("src/calc.rs::add".to_string(), 2);
    snap.dead_symbols.insert("src/calc.rs::unused".to_string());
    snap.public_api.insert("src/calc.rs::add".to_string());

    let text = serialize_snapshot_to_string(&snap);
    assert!(text.contains("head abcdef1234567890abcdef1234567890abcdef12"));
    assert!(text.contains("dirty 1"));
    assert!(text.contains("ccx src/calc.rs::add 3"));

    let parsed = parse_snapshot_from_str(&text);
    assert_eq!(parsed.head_sha, snap.head_sha);
    assert!(parsed.dirty);
    assert_eq!(parsed.absorbed, 2);
    assert_eq!(parsed.ccx_by_sym["src/calc.rs::add"], 3);
    assert_eq!(parsed.params_by_sym["src/calc.rs::add"], 2);
    assert!(parsed.dead_symbols.contains("src/calc.rs::unused"));
    assert!(parsed.public_api.contains("src/calc.rs::add"));
}

fn dead_symbols(files: &[(&str, ripwire_rs::Lang, &str)]) -> Vec<String> {
    use ripwire_rs::{Graph, Interner, LanguageRegistry, ingest_in_memory_files};
    let registry = LanguageRegistry::new().unwrap_or_else(|e| panic!("registry: {e:?}"));
    let interner = Interner::new();
    let slices: Vec<_> = files
        .iter()
        .map(|(p, l, s)| (*p, *l, s.as_bytes()))
        .collect();
    let ingest = ingest_in_memory_files(&slices, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    let snap = ripwire_rs::gate::baseline::compute_snapshot(&ingest, &graph, &interner, "head");
    let mut dead: Vec<String> = snap.dead_symbols.into_iter().collect();
    dead.sort_unstable();
    dead
}

#[test]
fn test_snapshot_dead_symbols_follow_each_language_dispatch_rules() {
    use ripwire_rs::Lang::{Python, Rust, TypeScript};
    let rust = "fn orphan() -> u32 {\n    1\n}\n\nfn main() {}\n\nstruct Store;\n\nimpl Store {\n    fn size(&self) -> u32 {\n        2\n    }\n}\n\nstruct Shown;\n\nimpl std::fmt::Display for Shown {\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n        write!(f, \"x\")\n    }\n}\n";
    let ts = "export class Client {\n  constructor() {}\n}\n\nexport function unused(): number {\n  return 1;\n}\n\nexport const tool = {\n  execute() {\n    return 2;\n  },\n};\n";
    let store_test = "#[test]\nfn store_is_small() {\n    assert!(true);\n}\n";
    let shown = "struct Shown;\n\nimpl std::fmt::Display for Shown {\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n        write!(f, \"x\")\n    }\n}\n\nimpl Shown {\n    fn stale(&self) -> u32 {\n        1\n    }\n}\n";
    let disk = "interface Store {\n  load(): number;\n}\n\nexport class Disk implements Store {\n  load(): number {\n    return 1;\n  }\n\n  unused(): number {\n    return 2;\n  }\n}\n";
    let echo = "from base import Handler\n\n\nclass Echo(Handler):\n    def handle(self):\n        return 1\n\n    def __repr__(self):\n        return \"echo\"\n\n\nclass Plain:\n    def orphan(self):\n        return 2\n";
    let blog = "from flask import Blueprint\n\nbp = Blueprint(\"blog\", __name__)\n\n\n@bp.route(\"/\")\ndef index():\n    return \"ok\"\n\n\n@staticmethod\ndef helper():\n    return 1\n\n\ndef __getattr__(name):\n    return name\n";
    let api = "export class Api {\n  @Get(\"/\")\n  list(): number {\n    return 1;\n  }\n\n  stale(): number {\n    return 2;\n  }\n}\n";
    type Case<'a> = (&'a [(&'a str, ripwire_rs::Lang, &'a str)], &'a [&'a str]);
    let cases: [Case<'_>; 3] = [
        (
            &[
                ("src/store.rs", Rust, rust),
                ("src/client.ts", TypeScript, ts),
                ("tests/store_test.rs", Rust, store_test),
            ],
            &[
                "src/client.ts::unused",
                "src/store.rs::Store::size",
                "src/store.rs::orphan",
            ],
        ),
        (
            &[
                ("src/shown.rs", Rust, shown),
                ("src/disk.ts", TypeScript, disk),
                ("src/echo.py", Python, echo),
            ],
            &[
                "src/disk.ts::Disk::unused",
                "src/echo.py::Plain::orphan",
                "src/shown.rs::Shown::stale",
            ],
        ),
        (
            &[
                ("src/blog.py", Python, blog),
                ("src/api.ts", TypeScript, api),
            ],
            &["src/api.ts::Api::stale", "src/blog.py::helper"],
        ),
    ];
    for (files, expected) in cases {
        assert_eq!(dead_symbols(files), expected);
    }
}

#[test]
fn test_python_local_definition_of_the_same_name_does_not_keep_a_symbol_alive() {
    use ripwire_rs::Lang::Python;
    let probe = "def p1(a):\n    return a - 1\n";
    let local = "def test_it():\n    def p1(now):\n        return now\n\n    handler = p1\n    return handler\n";
    let files = [
        ("src/probe.py", Python, probe),
        ("tests/test_local.py", Python, local),
    ];
    assert_eq!(dead_symbols(&files), ["src/probe.py::p1"]);
}

#[test]
fn test_python_import_of_the_symbol_keeps_it_alive() {
    use ripwire_rs::Lang::Python;
    let lib = "def helper(a):\n    return a + 1\n";
    let user = "from src.lib import helper\n\n\nregistry = {\"h\": helper}\n";
    let files = [("src/lib.py", Python, lib), ("src/user.py", Python, user)];
    assert!(dead_symbols(&files).is_empty());
}

#[test]
fn test_python_local_binding_of_the_same_name_does_not_keep_a_symbol_alive() {
    use ripwire_rs::Lang::Python;
    let probe = "def p1(a):\n    return a - 1\n\n\ndef p3(a):\n    return a - 3\n\n\ndef p4(a):\n    return a - 4\n";
    let assigned = "def test_it(rule):\n    p1 = rule(1)\n    return p1(2)\n";
    let unpacked = "def test_it(rule):\n    for p3, other in rule():\n        p3(other)\n";
    let other_function = "from src.probe import p4\n\n\ndef bind(rule):\n    p4 = rule(1)\n    return p4\n\n\ndef use():\n    return p4(1)\n\n\ndef main():\n    bind(None)\n    return use()\n";
    let files = [
        ("src/probe.py", Python, probe),
        ("tests/test_assigned.py", Python, assigned),
        ("tests/test_unpacked.py", Python, unpacked),
        ("src/other.py", Python, other_function),
    ];
    assert_eq!(
        dead_symbols(&files),
        ["src/probe.py::p1", "src/probe.py::p3"]
    );
}

#[test]
fn test_python_calls_bound_to_another_definition_do_not_keep_a_symbol_alive() {
    use ripwire_rs::Lang::Python;
    let probe = "def classify(a):\n    return a - 1\n\n\ndef grade(a):\n    return a - 2\n";
    let real = "def classify(a):\n    return a * 2\n\n\ndef grade(a):\n    return a * 3\n";
    let user = "from src.real import classify\nimport src.real as real\n\n\ndef main():\n    return classify(1) + real.classify(2)\n";
    let kept = "from src.probe import grade\n\n\ndef main():\n    return grade(1)\n";
    let files = [
        ("src/probe.py", Python, probe),
        ("src/real.py", Python, real),
        ("src/user.py", Python, user),
        ("src/kept.py", Python, kept),
    ];
    assert_eq!(
        dead_symbols(&files),
        ["src/probe.py::classify", "src/real.py::grade"]
    );
}

#[test]
fn test_python_method_reached_through_a_typed_call_to_its_protocol_stays_alive() {
    use ripwire_rs::Lang::Python;
    let steps = "from typing import Protocol\n\n\nclass Steps(Protocol):\n    def fetch(self) -> int: ...\n";
    let impl_ = "class Impl:\n    def fetch(self) -> int:\n        return 1\n";
    let user = "from src.steps import Steps\n\n\ndef run(steps: Steps) -> int:\n    return steps.fetch()\n\n\ndef main():\n    return run(None)\n";
    let files = [
        ("src/steps.py", Python, steps),
        ("src/impl.py", Python, impl_),
        ("src/user.py", Python, user),
    ];
    assert!(dead_symbols(&files).is_empty());
}

#[test]
fn test_python_imports_bind_through_package_roots_and_relative_levels() {
    use ripwire_rs::Lang::Python;
    let decoy = "def classify(a):\n    return a - 1\n";
    let real = "def classify(a):\n    return a * 2\n";
    type Case<'a> = (&'a str, &'a [(&'a str, &'a str)]);
    let cases: [Case<'_>; 3] = [
        (
            "package root above the importer",
            &[
                ("svc/src/pkg/real.py", real),
                (
                    "svc/src/pkg/user.py",
                    "from pkg.real import classify\n\n\ndef main():\n    return classify(1)\n",
                ),
            ],
        ),
        (
            "tests importing the service through its src directory",
            &[
                ("svc/src/real.py", real),
                (
                    "svc/tests/test_user.py",
                    "from src.real import classify\n\n\ndef main():\n    return classify(1)\n",
                ),
            ],
        ),
        (
            "relative import two levels up",
            &[
                ("svc/pkg/real.py", real),
                (
                    "svc/pkg/sub/user.py",
                    "from ..real import classify\n\n\ndef main():\n    return classify(1)\n",
                ),
            ],
        ),
    ];
    for (name, files) in cases {
        let mut all: Vec<(&str, ripwire_rs::Lang, &str)> =
            files.iter().map(|&(p, s)| (p, Python, s)).collect();
        all.push(("other/decoy.py", Python, decoy));
        assert_eq!(dead_symbols(&all), ["other/decoy.py::classify"], "{name}");
    }
}

#[test]
fn test_python_module_conforming_to_a_protocol_stays_alive_behind_a_receiver_call() {
    use ripwire_rs::Lang::Python;
    let protocol = "from typing import Protocol\n\n\nclass Runtime(Protocol):\n    def configure(self, mode: str) -> None: ...\n";
    let module = "def configure(mode: str) -> None:\n    return None\n";
    let user = "from src.protocol import Runtime\n\n\ndef run(runtime: Runtime) -> None:\n    runtime.configure(\"x\")\n\n\ndef main():\n    return run(None)\n";
    let files = [
        ("src/protocol.py", Python, protocol),
        ("src/module.py", Python, module),
        ("src/user.py", Python, user),
    ];
    assert_eq!(dead_symbols(&files), Vec::<String>::new());
}

#[test]
fn test_python_module_stays_alive_behind_a_call_on_an_untyped_receiver() {
    use ripwire_rs::Lang::Python;
    let protocol = "from typing import Protocol\n\n\nclass Runtime(Protocol):\n    def configure(self, mode: str) -> None: ...\n\n\ndef _runtime() -> Runtime:\n    import src.module as module\n\n    return module\n\n\ndef main():\n    _runtime().configure(\"x\")\n";
    let module = "def configure(mode: str) -> None:\n    return None\n";
    let files = [
        ("src/protocol.py", Python, protocol),
        ("src/module.py", Python, module),
    ];
    assert_eq!(dead_symbols(&files), Vec::<String>::new());
}

#[test]
fn test_python_function_is_not_kept_alive_by_a_call_in_another_language() {
    use ripwire_rs::Lang::{Python, TypeScript};
    let py = "def classify(a):\n    return a - 1\n";
    let ts = "export function run(x: number): number {\n  return x.classify(1);\n}\n";
    let files = [("src/probe.py", Python, py), ("web/run.ts", TypeScript, ts)];
    assert_eq!(
        dead_symbols(&files),
        ["src/probe.py::classify", "web/run.ts::run"]
    );
}

#[test]
fn test_python_value_use_of_an_imported_definition_does_not_keep_another_alive() {
    use ripwire_rs::Lang::Python;
    let probe = "def worker(a):\n    return a - 1\n\n\ndef task(a):\n    return a - 2\n";
    let real = "def worker(a):\n    return a * 2\n\n\ndef task(a):\n    return a * 3\n";
    let user = "from src.real import worker\n\n\ndef main(queue):\n    queue.add(worker)\n";
    let kept = "from src.probe import task\n\n\ndef main(queue):\n    queue.add(task)\n";
    let files = [
        ("src/probe.py", Python, probe),
        ("src/real.py", Python, real),
        ("src/user.py", Python, user),
        ("src/kept.py", Python, kept),
    ];
    assert_eq!(
        dead_symbols(&files),
        ["src/probe.py::worker", "src/real.py::task"]
    );
}

#[test]
fn test_python_from_import_of_a_submodule_binds_qualified_calls() {
    use ripwire_rs::Lang::Python;
    let probe = "def classify(a):\n    return a - 1\n";
    let real = "def classify(a):\n    return a * 2\n";
    let plain = "from src import real\n\n\ndef main():\n    return real.classify(2)\n";
    let aliased = "from src import real as r\n\n\ndef main():\n    return r.classify(2)\n";
    let relative = "from . import real\n\n\ndef main():\n    return real.classify(2)\n";
    for (user_path, user) in [
        ("app/user.py", plain),
        ("app/aliased.py", aliased),
        ("src/relative.py", relative),
    ] {
        let files = [
            ("src/probe.py", Python, probe),
            ("src/real.py", Python, real),
            (user_path, Python, user),
        ];
        assert_eq!(dead_symbols(&files), ["src/probe.py::classify"], "{user}");
    }
}

#[test]
fn test_python_call_bound_to_python_does_not_keep_a_symbol_of_another_language_alive() {
    use ripwire_rs::Lang::{Python, Rust};
    let rust = "pub fn helper() -> u32 {\n    1\n}\n";
    let python = "def helper():\n    return 1\n\n\ndef main():\n    return helper()\n";
    let files = [("src/lib.rs", Rust, rust), ("app/main.py", Python, python)];
    assert_eq!(dead_symbols(&files), ["src/lib.rs::helper"]);
}

#[test]
fn test_typescript_block_scoped_binding_shadows_only_its_block() {
    use ripwire_rs::Lang::TypeScript;
    let decoy = "export function p1(n: number) {\n  return n;\n}\n";
    let user = "import { p1 } from \"./decoy\";\n\nexport function main(make: () => number) {\n  if (make()) {\n    const p1 = make();\n    return p1;\n  }\n  return p1(1);\n}\n";
    let files = [
        ("app/src/decoy.ts", TypeScript, decoy),
        ("app/src/user.ts", TypeScript, user),
    ];
    assert_eq!(dead_symbols(&files), Vec::<String>::new());
}

#[test]
fn test_typescript_dynamic_import_reaches_only_the_names_it_uses() {
    use ripwire_rs::Lang::TypeScript;
    let panel = "export default function Panel() {\n  return null;\n}\n\nexport function Foo() {\n  return 1;\n}\n\nexport function Bar() {\n  return 2;\n}\n";
    type Case<'a> = (&'a str, &'a str, &'a [&'a str]);
    let cases: [Case<'_>; 5] = [
        (
            "lazy loads the default export",
            "import { lazy } from \"react\";\n\nexport const Lazy = lazy(() => import(\"./panel\"));\n",
            &["app/src/panel.tsx::Bar", "app/src/panel.tsx::Foo"],
        ),
        (
            "then picks one export",
            "export const picked = import(\"./panel\").then((m) => ({ default: m.Foo }));\n",
            &["app/src/panel.tsx::Bar", "app/src/panel.tsx::Panel"],
        ),
        (
            "then destructures its parameter",
            "export const picked = import(\"./panel\").then(({ Bar }) => Bar);\n",
            &["app/src/panel.tsx::Foo", "app/src/panel.tsx::Panel"],
        ),
        (
            "await destructures the module",
            "export const { Bar } = await import(\"./panel\");\n",
            &["app/src/panel.tsx::Foo", "app/src/panel.tsx::Panel"],
        ),
        (
            "an opaque module reaches everything",
            "export const whole = await import(\"./panel\");\n",
            &[],
        ),
    ];
    for (name, user, dead) in cases {
        let files = [
            ("app/src/panel.tsx", TypeScript, panel),
            ("app/src/user.tsx", TypeScript, user),
        ];
        assert_eq!(dead_symbols(&files), dead, "{name}");
    }
}

#[test]
fn test_sql_function_called_from_an_embedded_query_stays_alive() {
    use ripwire_rs::Lang::{Python, Sql};
    let sql = "CREATE FUNCTION public.promote_template(p uuid) RETURNS integer LANGUAGE sql AS $$ SELECT 1 $$;\n\nCREATE FUNCTION public.unused_function(p uuid) RETURNS integer LANGUAGE sql AS $$ SELECT 2 $$;\n";
    let python = "def promote(session):\n    return session.execute(\"SELECT promote_template(CAST(:p AS uuid))\")\n\n\ndef main(session):\n    return promote(session)\n";
    let files = [
        ("db/functions.sql", Sql, sql),
        ("app/repo.py", Python, python),
    ];
    assert_eq!(dead_symbols(&files), ["db/functions.sql::unused_function"]);
}

#[test]
fn test_sql_function_called_through_a_supabase_rpc_stays_alive() {
    use ripwire_rs::Lang::{Sql, TypeScript};
    let sql = "CREATE FUNCTION public.promote_template(p uuid) RETURNS integer LANGUAGE sql AS $$ SELECT 1 $$;\n\nCREATE FUNCTION public.unused_function(p uuid) RETURNS integer LANGUAGE sql AS $$ SELECT 2 $$;\n\nCREATE FUNCTION public.is_admin() RETURNS boolean LANGUAGE sql AS $$ SELECT true $$;\n";
    let ts = "export async function promote(supabase: Client) {\n  const { data } = await supabase.rpc(\n    \"promote_template\",\n    { p: 1 },\n  );\n  supabase.rpc('is_admin').then(() => 1);\n  return data;\n}\n\nexport async function main(supabase: Client) {\n  return promote(supabase);\n}\n";
    let files = [
        ("db/functions.sql", Sql, sql),
        ("web/src/api.ts", TypeScript, ts),
    ];
    assert_eq!(dead_symbols(&files), ["db/functions.sql::unused_function"]);
}

#[test]
fn test_snapshot_counts_a_variadic_parameter_as_one() {
    use ripwire_rs::Lang::{Python, TypeScript};
    use ripwire_rs::{Graph, Interner, LanguageRegistry, ingest_in_memory_files};
    let registry = LanguageRegistry::new().unwrap_or_else(|e| panic!("registry: {e:?}"));
    let interner = Interner::new();
    let py = "def rest(*args):\n    return args\n\n\ndef mixed(a, *args, **kwargs):\n    return a\n\n\ndef plain(a, b):\n    return a\n";
    let ts = "export function join(...inputs: string[]) {\n  return inputs;\n}\n\nexport function both(a: number, ...rest: number[]) {\n  return a;\n}\n";
    let files = [
        ("src/a.py", Python, py.as_bytes()),
        ("src/b.ts", TypeScript, ts.as_bytes()),
    ];
    let ingest = ingest_in_memory_files(&files, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    let snap = ripwire_rs::gate::baseline::compute_snapshot(&ingest, &graph, &interner, "head");
    let params = |key: &str| snap.params_by_sym.get(key).copied();
    assert_eq!(
        (
            params("src/a.py::rest"),
            params("src/a.py::mixed"),
            params("src/a.py::plain"),
            params("src/b.ts::join"),
            params("src/b.ts::both"),
        ),
        (Some(1), Some(3), Some(2), Some(1), Some(2))
    );
}

#[test]
fn test_sql_function_called_through_a_python_rpc_stays_alive() {
    use ripwire_rs::Lang::{Python, Sql};
    let sql = "CREATE FUNCTION public.promote_template(p uuid) RETURNS integer LANGUAGE sql AS $$ SELECT 1 $$;\n\nCREATE FUNCTION public.unused_function(p uuid) RETURNS integer LANGUAGE sql AS $$ SELECT 2 $$;\n\nCREATE FUNCTION public.by_variable(p uuid) RETURNS integer LANGUAGE sql AS $$ SELECT 3 $$;\n";
    let python = "def promote(client):\n    return client.rpc(\n        \"promote_template\",\n        {\"p\": 1},\n    ).execute()\n\n\ndef dynamic(client, name):\n    return client.rpc(name, {}).execute()\n\n\ndef main(client):\n    dynamic(client, \"by_variable\")\n    return promote(client)\n";
    let files = [
        ("db/functions.sql", Sql, sql),
        ("app/repo.py", Python, python),
    ];
    assert_eq!(
        dead_symbols(&files),
        [
            "db/functions.sql::by_variable",
            "db/functions.sql::unused_function"
        ]
    );
}

#[test]
fn test_typescript_name_imported_from_an_external_package_does_not_keep_a_symbol_alive() {
    use ripwire_rs::Lang::TypeScript;
    let probe = "export function cn(...inputs: string[]) {\n  return inputs.join(\" \");\n}\n";
    let external = "import { cn } from \"@acme/ui/cn\";\n\nexport function Avatar() {\n  return cn(\"a\");\n}\n";
    let local =
        "import { cn } from \"./probe\";\n\nexport function Chip() {\n  return cn(\"a\");\n}\n";
    let files = [
        ("app/src/probe.ts", TypeScript, probe),
        ("app/src/Avatar.tsx", TypeScript, external),
    ];
    assert_eq!(
        dead_symbols(&files),
        ["app/src/Avatar.tsx::Avatar", "app/src/probe.ts::cn"]
    );
    let files = [
        ("app/src/probe.ts", TypeScript, probe),
        ("app/src/Chip.tsx", TypeScript, local),
    ];
    assert_eq!(dead_symbols(&files), ["app/src/Chip.tsx::Chip"]);
}

#[test]
fn test_renamed_imports_keep_the_original_definition_alive() {
    use ripwire_rs::Lang::{Python, TypeScript};
    let ts_probe = "export function cn(...inputs: string[]) {\n  return inputs.join(\" \");\n}\n";
    let ts_user = "import { cn as join } from \"./probe\";\n\nexport function Chip() {\n  return join(\"a\");\n}\n";
    let files = [
        ("app/src/probe.ts", TypeScript, ts_probe),
        ("app/src/Chip.tsx", TypeScript, ts_user),
    ];
    assert_eq!(dead_symbols(&files), ["app/src/Chip.tsx::Chip"]);
    let py_probe = "def classify(a):\n    return a - 1\n";
    let py_user = "from src.probe import classify as grade\n\n\ndef main():\n    return grade(1)\n";
    let files = [
        ("src/probe.py", Python, py_probe),
        ("src/user.py", Python, py_user),
    ];
    assert_eq!(dead_symbols(&files), Vec::<String>::new());
}

#[test]
fn test_renamed_import_from_an_outside_package_does_not_keep_a_symbol_alive() {
    use ripwire_rs::Lang::TypeScript;
    let probe = "export function cn(...inputs: string[]) {\n  return inputs.join(\" \");\n}\n";
    let external = "import { cn as join } from \"@acme/ui/cn\";\n\nexport function Avatar() {\n  return join(\"a\");\n}\n";
    let files = [
        ("app/src/probe.ts", TypeScript, probe),
        ("app/src/Avatar.tsx", TypeScript, external),
    ];
    assert_eq!(
        dead_symbols(&files),
        ["app/src/Avatar.tsx::Avatar", "app/src/probe.ts::cn"]
    );
}
