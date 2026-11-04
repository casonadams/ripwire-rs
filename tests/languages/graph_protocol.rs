use super::harness::{GraphProtocolCase, sym, verify_protocol_case};
use ripwire_rs::{Lang, SymKind};

type Entrypoint = (&'static str, Option<&'static str>, SymKind, &'static str);

#[test]
fn graph_protocol_cases_hold_for_each_language() {
    let rows: [(GraphProtocolCase<'_>, Option<Entrypoint>); 7] = [
        (
            GraphProtocolCase {
                label: "csharp",
                files: &[(
                    "src/Repo.cs",
                    Lang::CSharp,
                    b"using System.IO;\npublic interface IStore {}\npublic class SqlStore : IStore {\n    public const int LIMIT = 50;\n    private int retryCounter = 0;\n    public void Save() {\n        Console.WriteLine();\n        Commit();\n    }\n    private void Commit() {\n        return;\n    }\n}\n",
                )],
                expected_syms: &[
                    sym("IStore", SymKind::Interface, None, true),
                    sym("SqlStore", SymKind::Class, None, true),
                    sym("LIMIT", SymKind::Var, Some("SqlStore"), false),
                    sym("Save", SymKind::Method, Some("SqlStore"), true),
                    sym("Commit", SymKind::Method, Some("SqlStore"), true),
                ],
                rejected_syms: &["retryCounter"],
                expected_edges: &[("Save", "Commit")],
                expect_external: true,
                expected_lego: Some(("IStore", "SqlStore")),
                expected_import: Some("IO"),
            },
            Some(("Main", Some("Program"), SymKind::Method, "src/Program.cs")),
        ),
        (
            GraphProtocolCase {
                label: "java",
                files: &[(
                    "src/Worker.java",
                    Lang::Java,
                    b"import java.util.List;\npublic interface Job { void runJob(); }\npublic class Worker implements Job {\n    public static final int MAX_Q = 10;\n    private int instanceField = 1;\n    public void runJob() {\n        Collections.emptyList();\n        helperStep();\n    }\n    private void helperStep() {\n        return;\n    }\n}\n",
                )],
                expected_syms: &[
                    sym("Job", SymKind::Interface, None, true),
                    sym("Worker", SymKind::Class, None, true),
                    sym("MAX_Q", SymKind::Var, Some("Worker"), false),
                    sym("runJob", SymKind::Method, Some("Worker"), true),
                    sym("helperStep", SymKind::Method, Some("Worker"), true),
                ],
                rejected_syms: &["instanceField"],
                expected_edges: &[("runJob", "helperStep")],
                expect_external: true,
                expected_lego: Some(("Job", "Worker")),
                expected_import: Some("List"),
            },
            Some(("main", Some("Worker"), SymKind::Method, "src/Worker.java")),
        ),
        (
            GraphProtocolCase {
                label: "hcl",
                files: &[(
                    "infra/main.tf",
                    Lang::Hcl,
                    b"resource \"aws_s3_bucket\" \"primary\" {\n  bucket = format(\"%s\", var.name)\n}\n",
                )],
                expected_syms: &[
                    sym("primary", SymKind::Class, None, false),
                    sym("aws_s3_bucket", SymKind::Section, None, false),
                    sym("bucket", SymKind::Field, None, false),
                ],
                rejected_syms: &["resource"],
                ..Default::default()
            },
            None,
        ),
        (
            GraphProtocolCase {
                label: "json",
                files: &[(
                    "package.json",
                    Lang::Json,
                    b"{\n  \"name\": \"pkg\",\n  \"scripts\": {\n    \"build\": \"cargo build\",\n    \"nested\": { \"deep_leaf\": true }\n  }\n}\n",
                )],
                expected_syms: &[
                    sym("name", SymKind::Section, None, false),
                    sym("scripts", SymKind::Section, None, false),
                    sym("build", SymKind::Section, None, false),
                ],
                rejected_syms: &["deep_leaf"],
                ..Default::default()
            },
            None,
        ),
        (
            GraphProtocolCase {
                label: "graphql",
                files: &[(
                    "schema.graphql",
                    Lang::Graphql,
                    b"interface Node {\n  id: ID!\n}\ntype Account {\n  id: ID!\n}\n",
                )],
                expected_syms: &[
                    sym("Node", SymKind::Interface, None, false),
                    sym("Account", SymKind::Class, None, false),
                    sym("id", SymKind::Field, None, false),
                ],
                ..Default::default()
            },
            None,
        ),
        (
            GraphProtocolCase {
                label: "proto",
                files: &[(
                    "api.proto",
                    Lang::Proto,
                    b"syntax = \"proto3\";\nservice AuthService {\n  rpc Login (LoginReq) returns (LoginResp);\n}\nmessage LoginReq {\n  string user = 1;\n}\n",
                )],
                expected_syms: &[
                    sym("AuthService", SymKind::Class, None, false),
                    sym("Login", SymKind::Function, None, false),
                    sym("LoginReq", SymKind::Class, None, false),
                ],
                ..Default::default()
            },
            None,
        ),
        (
            GraphProtocolCase {
                label: "cuda",
                files: &[(
                    "src/kernel.cu",
                    Lang::Cuda,
                    b"#include \"kernel.cuh\"\n__constant__ float d_weights[16];\nvoid launch(void) {\n    run_kernel();\n}\n__global__ void run_kernel(void) {\n    return;\n}\n",
                )],
                expected_syms: &[
                    sym("d_weights", SymKind::Var, None, false),
                    sym("launch", SymKind::Function, None, true),
                    sym("run_kernel", SymKind::Function, None, true),
                ],
                expected_edges: &[("launch", "run_kernel")],
                expected_import: Some("kernel.cuh"),
                ..Default::default()
            },
            None,
        ),
    ];
    for (case, entrypoint) in &rows {
        verify_protocol_case(case);
        if let Some((name, owner, kind, path)) = entrypoint {
            let (_, lang, _) = case.files[0];
            assert!(
                lang.semantics()
                    .is_entrypoint_symbol(name, *owner, *kind, path),
                "{}: entrypoint {name} not recognised",
                case.label
            );
        }
    }
}
