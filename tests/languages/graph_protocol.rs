use super::harness::{GraphProtocolCase, NO_CASE, sym, verify_protocol_case};
use ripwire_rs::{Lang, SymKind};
use std::collections::HashMap;

type Entrypoint = (&'static str, Option<&'static str>, SymKind, &'static str);
type StdCase = (
    Lang,
    &'static [&'static str],
    &'static [&'static str],
    &'static [&'static str],
);

const ROWS: &[(GraphProtocolCase<'static>, Option<Entrypoint>)] = &[
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
..NO_CASE
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
..NO_CASE
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
..NO_CASE
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
..NO_CASE
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
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "bash",
        files: &[(
            "scripts/deploy.sh",
            Lang::Bash,
            b"#!/usr/bin/env bash\nrun_deploy() {\n  LOCAL_VAR=1\n  build_image\n}\nbuild_image() {\n  echo ok\n}\n",
        )],
        expected_syms: &[
            sym("run_deploy", SymKind::Function, None, true),
            sym("build_image", SymKind::Function, None, true),
        ],
        rejected_syms: &["LOCAL_VAR"],
        expected_edges: &[("run_deploy", "build_image")],
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "brightscript",
        files: &[
            (
                "source/main.bs",
                Lang::BrightScript,
                b"library \"pkg:/source/utils.brs\"\n\nsub main()\n    initApp()\n    m.runLoop()\nend sub\n\nfunction initApp() as object\n    helper()\n    return invalid\nend function\n",
            ),
            (
                "source/utils.brs",
                Lang::BrightScript,
                b"sub helper()\n    print \"helper\"\nend sub\n",
            ),
        ],
        expected_syms: &[
            sym("main", SymKind::Function, None, true),
            sym("initApp", SymKind::Function, None, true),
            sym("helper", SymKind::Function, None, true),
        ],
        expected_edges: &[
            ("main", "initApp"),
            ("initApp", "helper"),
        ],
        expected_import: Some("pkg:/source/utils.brs"),
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "c",
        files: &[(
            "src/core.c",
            Lang::C,
            b"#include \"core.h\"\n#define EMPTY_FN(x)\nstatic const int MAX_CONN = 16;\nint plain_global = 0;\nvoid start_server(void) {\n    accept_loop();\n}\nvoid accept_loop(void) {\n    return;\n}\n",
        )],
        expected_syms: &[
            sym("MAX_CONN", SymKind::Var, None, false),
            sym("start_server", SymKind::Function, None, true),
            sym("accept_loop", SymKind::Function, None, true),
        ],
        rejected_syms: &["EMPTY_FN", "plain_global"],
        expected_edges: &[("start_server", "accept_loop")],
        expected_import: Some("core.h"),
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
                label: "cobol",
                files: &[("src/payroll.cbl", Lang::Cobol, b"       IDENTIFICATION DIVISION.\n       PROGRAM-ID. PAYROLL.\n       PROCEDURE DIVISION.\n       MAIN-LOGIC.\n           PERFORM CALC-TAX.\n           STOP RUN.\n       CALC-TAX.\n           DISPLAY \"CALCULATING\".\n")],
                expected_syms: &[
                    sym("MAIN-LOGIC", SymKind::Function, None, false),
                    sym("CALC-TAX", SymKind::Function, None, false),
                ],
                expected_edges: &[("MAIN-LOGIC", "CALC-TAX")],
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "cpp",
        files: &[(
            "src/engine.cpp",
            Lang::Cpp,
            b"#include \"engine.h\"\n#define NOOP_MACRO(x)\nstatic const int MAX_THREADS = 8;\nconstexpr int kValidConst = 7;\nint mutable_global = 0;\nint uninitialized_global;\nclass BaseEngine {\npublic:\n    static constexpr int kClassConst = 9;\n    static int s_mutable;\n    int count;\n};\ntemplate<> class BaseEngine<int> {};\nclass RenderEngine : public BaseEngine {\npublic:\n    void render() {\n        std::lock_guard<std::mutex> guard(m);\n        std::move(0);\n        flushBuffers();\n    }\n};\nvoid flushBuffers() {\n    return;\n}\nTEST_CASE(\"suite\") {\n    flushBuffers();\n}\n",
        )],
        expected_syms: &[
            sym("MAX_THREADS", SymKind::Var, None, false),
            sym("kValidConst", SymKind::Var, None, false),
            sym("kClassConst", SymKind::Var, Some("BaseEngine"), false),
            sym("count", SymKind::Field, Some("BaseEngine"), false),
            sym("RenderEngine", SymKind::Class, None, true),
            sym("render", SymKind::Method, Some("RenderEngine"), true),
            sym("flushBuffers", SymKind::Function, None, true),
            sym("TEST_CASE", SymKind::Macro, None, true),
        ],
        rejected_syms: &["NOOP_MACRO", "mutable_global", "uninitialized_global", "s_mutable", "guard"],
        expected_edges: &[("render", "flushBuffers")],
        expect_external: true,
        expected_lego: Some(("BaseEngine", "RenderEngine")),
        expected_import: Some("engine.h"),
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "dart",
        files: &[(
            "lib/service.dart",
            Lang::Dart,
            b"import 'package:core/core.dart';\nabstract class Runner {}\nclass TaskRunner extends Runner {\n  static List<String> allCols() => [];\n  void execute() {\n    print('skip');\n    Future.wait([]);\n    performStep();\n  }\n  void performStep() {\n    return;\n  }\n}\n",
        )],
        expected_syms: &[
            sym("Runner", SymKind::Interface, None, true),
            sym("TaskRunner", SymKind::Class, None, true),
            sym("allCols", SymKind::Method, Some("TaskRunner"), true),
            sym("execute", SymKind::Method, Some("TaskRunner"), true),
            sym("performStep", SymKind::Method, Some("TaskRunner"), true),
        ],
        expected_edges: &[("execute", "performStep")],
        expect_external: true,
        expected_lego: Some(("Runner", "TaskRunner")),
        expected_import: Some("package:core/core.dart"),
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "elixir",
        files: &[(
            "lib/pipeline.ex",
            Lang::Elixir,
            b"defmodule Pipeline do\n  @doc \"hidden\"\n  @custom_attr 42\n  @type state :: map()\n  def execute(x) do\n    Enum.map(x, &(&1))\n    transform(x)\n  end\n  defp transform(x) do\n    x\n  end\n  test \"runs pipeline\" do\n    execute(1)\n  end\nend\n",
        )],
        expected_syms: &[
            sym("Pipeline", SymKind::Section, None, true),
            sym("custom_attr", SymKind::Var, Some("Pipeline"), false),
            sym("state", SymKind::Var, Some("Pipeline"), false),
            sym("execute/1", SymKind::Function, Some("Pipeline"), true),
            sym("transform/1", SymKind::Function, Some("Pipeline"), true),
            sym("\"runs pipeline\"/0", SymKind::Function, Some("Pipeline"), true),
        ],
        rejected_syms: &["doc", "x/0"],
        expected_edges: &[("execute/1", "transform/1")],
        expect_external: true,
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "erlang",
        files: &[(
            "src/server.erl",
            Lang::Erlang,
            b"-module(server).\nstart() ->\n    lists:map(fun step/1, []),\n    step(1).\nstep(X) ->\n    X.\n",
        )],
        expected_syms: &[
            sym("start", SymKind::Function, None, true),
            sym("step", SymKind::Function, None, true),
        ],
        expected_edges: &[("start", "step")],
        expect_external: true,
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "fortran",
        files: &[(
            "src/math.f90",
            Lang::Fortran,
            b"module math_ops\ncontains\n  subroutine compute(x)\n    integer :: x\n    call helper(x)\n  end subroutine compute\n  subroutine helper(y)\n    integer :: y\n  end subroutine helper\nend module math_ops\n",
        )],
        expected_syms: &[
            sym("math_ops", SymKind::Section, None, false),
            sym("compute", SymKind::Function, None, false),
            sym("helper", SymKind::Function, None, false),
        ],
        expected_edges: &[("compute", "helper")],
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "gdscript",
        files: &[(
            "scripts/boss.gd",
            Lang::GDScript,
            b"class Actor:\n\tpass\nclass Boss:\n\textends Actor\n\tfunc tick():\n\t\tadvance()\n\tfunc advance():\n\t\tpass\n",
        )],
        expected_syms: &[
            sym("Actor", SymKind::Class, None, true),
            sym("Boss", SymKind::Class, None, true),
            sym("tick", SymKind::Method, Some("Boss"), true),
            sym("advance", SymKind::Method, Some("Boss"), true),
        ],
        expected_edges: &[("tick", "advance")],
        expected_lego: Some(("Actor", "Boss")),
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "go",
        files: &[(
            "pkg/server.go",
            Lang::Go,
            b"package pkg\nimport \"fmt\"\ntype Token = string\ntype Server struct{}\nfunc (s *Server) Serve() {\n\tfmt.Println(\"up\")\n\thandleConn()\n}\nfunc handleConn() {\n\treturn\n}\n",
        )],
        expected_syms: &[
            sym("Token", SymKind::Var, None, false),
            sym("Server", SymKind::Struct, None, false),
            sym("Serve", SymKind::Method, Some("Server"), true),
            sym("handleConn", SymKind::Function, None, true),
        ],
        expected_edges: &[("Serve", "handleConn")],
        expect_external: true,
        expected_import: Some("fmt"),
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "go-grouped-params",
        files: &[(
            "check.go",
            Lang::Go,
            b"package main\nfunc checkContains(t *T, got, expected string) {}\nfunc probe() {\n\tcheckContains(nil, \"a\", \"b\")\n}\n",
        )],
        expected_syms: &[
            sym("checkContains", SymKind::Function, None, false),
            sym("probe", SymKind::Function, None, false),
        ],
        expected_edges: &[("probe", "checkContains")],
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "hcl-labels",
        files: &[(
            "infra/main.tf",
            Lang::Hcl,
            b"resource \"aws_s3_bucket\" \"logs\" {\n  bucket = \"a\"\n}\n\nresource \"aws_s3_bucket\" \"audit\" {\n  bucket = \"b\"\n}\n\nmodule \"network\" {\n  source = \"./net\"\n}\n\nlocals {\n  x = 1\n}\n",
        )],
        expected_syms: &[
            sym("logs", SymKind::Class, None, false),
            sym("audit", SymKind::Class, None, false),
            sym("network", SymKind::Class, None, false),
            sym("locals", SymKind::Class, None, false),
            sym("aws_s3_bucket", SymKind::Section, None, false),
        ],
        rejected_syms: &["resource"],
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "java-record",
        files: &[(
            "src/Points.java",
            Lang::Java,
            b"public record Point(int x, int y) {\n    public int sum() { return x + y; }\n}\n@interface Marker { String value(); }\n",
        )],
        expected_syms: &[
            sym("Point", SymKind::Class, None, true),
            sym("sum", SymKind::Method, Some("Point"), true),
            sym("Marker", SymKind::Interface, None, true),
        ],
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "javascript",
        files: &[(
            "src/app.js",
            Lang::JavaScript,
            b"import './init.js';\nconst API_VERSION = 2;\nconst tempLocal = 0;\nclass BaseCtrl {}\nclass UserCtrl extends BaseCtrl {\n  process() {\n    JSON.stringify({});\n    validate();\n  }\n}\nfunction validate() {\n  return true;\n}\n",
        )],
        expected_syms: &[
            sym("API_VERSION", SymKind::Var, None, false),
            sym("UserCtrl", SymKind::Class, None, true),
            sym("process", SymKind::Method, Some("UserCtrl"), true),
            sym("validate", SymKind::Function, None, true),
        ],
        rejected_syms: &["tempLocal"],
        expected_edges: &[("process", "validate")],
        expect_external: true,
        expected_lego: Some(("BaseCtrl", "UserCtrl")),
        expected_import: Some("./init.js"),
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "kotlin",
        files: &[(
            "src/Handler.kt",
            Lang::Kotlin,
            b"import com.example.Util\ninterface Handler\nclass HttpHandler : Handler {\n    fun handle() {\n        Collections.emptyList()\n        dispatchReq()\n    }\n    fun dispatchReq() {\n        return\n    }\n}\n",
        )],
        expected_syms: &[
            sym("Handler", SymKind::Interface, None, false),
            sym("HttpHandler", SymKind::Class, None, true),
            sym("handle", SymKind::Method, Some("HttpHandler"), true),
            sym("dispatchReq", SymKind::Method, Some("HttpHandler"), true),
        ],
        expected_edges: &[("handle", "dispatchReq")],
        expect_external: true,
        expected_lego: Some(("Handler", "HttpHandler")),
        expected_import: Some("Util"),
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "lua",
        files: &[(
            "src/mod.lua",
            Lang::Lua,
            b"local M = {}\nfunction M:run(x)\n  string.format('%s', x)\n  helper(x)\nend\nfunction helper(v)\n  return v\nend\nreturn M\n",
        )],
        expected_syms: &[
            sym("run", SymKind::Method, Some("M"), true),
            sym("helper", SymKind::Function, None, true),
        ],
        rejected_syms: &["M"],
        expected_edges: &[("run", "helper")],
        expect_external: true,
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "markdown",
        files: &[(
            "docs/GUIDE.md",
            Lang::Markdown,
            b"# Guide\nIntro\n## Setup\nSteps\n",
        )],
        expected_syms: &[
            sym("Guide", SymKind::Section, None, false),
            sym("Setup", SymKind::Section, Some("Guide"), false),
        ],
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "metal",
        files: &[(
            "src/shader.metal",
            Lang::Metal,
            b"#include \"shader.h\"\nvoid compute_pass() {\n    metal::sin(0.0);\n    shade_vertex();\n}\nvoid shade_vertex() {\n    return;\n}\n",
        )],
        expected_syms: &[
            sym("compute_pass", SymKind::Function, None, true),
            sym("shade_vertex", SymKind::Function, None, true),
        ],
        expected_edges: &[("compute_pass", "shade_vertex")],
        expect_external: true,
        expected_import: Some("shader.h"),
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "metal-qualifiers",
        files: &[(
            "src/pass.metal",
            Lang::Metal,
            b"#include \"device.h\"\nstruct Uniforms { float scale; };\nfloat scale_uv(float2 uv, constant Uniforms& u) {\n    return uv.x * u.scale;\n}\nkernel void add_one(device float* out [[buffer(0)]],\n                    uint gid [[thread_position_in_grid]]) {\n    out[gid] = scale_uv(float2(0.0), out[gid]);\n}\n",
        )],
        expected_syms: &[
            sym("Uniforms", SymKind::Class, None, false),
            sym("scale_uv", SymKind::Function, None, true),
            sym("add_one", SymKind::Function, None, true),
        ],
        expected_edges: &[("add_one", "scale_uv")],
        expected_import: Some("device.h"),
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "objc",
        files: &[(
            "src/Renderer.m",
            Lang::ObjC,
            b"#import \"Renderer.h\"\n@interface BaseRenderer\n@end\n@interface MeshRenderer : BaseRenderer\n@end\n@implementation MeshRenderer\n- (void)draw {\n    NSString *s;\n    initPipeline();\n}\n@end\nvoid initPipeline(void) {\n    return;\n}\n",
        )],
        expected_syms: &[
            sym("MeshRenderer", SymKind::Class, None, true),
            sym("draw", SymKind::Method, Some("MeshRenderer"), true),
            sym("initPipeline", SymKind::Function, None, true),
        ],
        expected_edges: &[("draw", "initPipeline")],
        expected_lego: Some(("BaseRenderer", "MeshRenderer")),
        expected_import: Some("Renderer.h"),
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "php",
        files: &[(
            "src/Auth.php",
            Lang::Php,
            b"<?php\nuse App\\Contracts\\Guard;\ninterface Guard {}\nclass TokenGuard implements Guard {\n    public function check() {\n        verifyToken();\n    }\n}\nfunction verifyToken() {\n    return true;\n}\n",
        )],
        expected_syms: &[
            sym("Guard", SymKind::Interface, None, true),
            sym("TokenGuard", SymKind::Class, None, true),
            sym("check", SymKind::Method, Some("TokenGuard"), true),
            sym("verifyToken", SymKind::Function, None, true),
        ],
        expected_edges: &[("check", "verifyToken")],
        expected_lego: Some(("Guard", "TokenGuard")),
        expected_import: Some("Guard"),
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "python",
        files: &[(
            "pkg/worker.py",
            Lang::Python,
            b"import os\nclass Mode(Enum):\n    FAST = 1\nclass BaseWorker:\n    pass\nclass TaskWorker(BaseWorker):\n    untyped_noise = 0\n    def execute(self, other):\n        self.job_id = 7\n        other.temp_val = 20\n        os.getcwd()\n        run_step()\ndef run_step():\n    return 1\nself.top_level = 30\n",
        )],
        expected_syms: &[
            sym("FAST", SymKind::Var, Some("Mode"), false),
            sym("TaskWorker", SymKind::Class, None, true),
            sym("execute", SymKind::Function, Some("TaskWorker"), true),
            sym("job_id", SymKind::Field, Some("TaskWorker"), false),
            sym("run_step", SymKind::Function, None, true),
        ],
        rejected_syms: &["untyped_noise", "temp_val", "top_level"],
        expected_edges: &[("execute", "run_step")],
        expect_external: true,
        expected_lego: Some(("BaseWorker", "TaskWorker")),
        expected_import: Some("os"),
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "ruby",
        files: &[(
            "lib/job.rb",
            Lang::Ruby,
            b"class BaseJob\nend\nclass MailJob < BaseJob\n  MAX_RETRY = 3\n  AliasType = String\n  def perform\n    File.read('a')\n    deliver()\n  end\n  def deliver\n    true\n  end\nend\n",
        )],
        expected_syms: &[
            sym("MailJob", SymKind::Class, None, false),
            sym("MAX_RETRY", SymKind::Var, Some("MailJob"), false),
            sym("perform", SymKind::Method, Some("MailJob"), true),
            sym("deliver", SymKind::Method, Some("MailJob"), true),
        ],
        rejected_syms: &["AliasType"],
        expected_edges: &[("perform", "deliver")],
        expect_external: true,
        expected_lego: Some(("BaseJob", "MailJob")),
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "rust",
        files: &[(
            "src/lib.rs",
            Lang::Rust,
            b"use crate::util::helper;\npub trait Runner { fn execute(&self); }\npub struct Engine;\nimpl Runner for Engine {\n    pub fn execute(&self) {\n        let local_noise = 1;\n        println!(\"{}\", local_noise);\n        std::mem::drop(local_noise);\n        step();\n    }\n}\npub fn step() {}\n",
        )],
        expected_syms: &[
            sym("Runner", SymKind::Interface, None, true),
            sym("Engine", SymKind::Class, None, false),
            sym("execute", SymKind::Method, Some("Engine"), true),
            sym("step", SymKind::Function, None, true),
        ],
        rejected_syms: &["local_noise"],
        expected_edges: &[("execute", "step")],
        expect_external: true,
        expected_lego: Some(("Runner", "Engine")),
        expected_import: Some("crate::util::helper"),
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "scala",
        files: &[(
            "src/Service.scala",
            Lang::Scala,
            b"trait BaseService {\n  def run(): Unit\n}\nobject MainService extends BaseService {\n  def run(): Unit = {\n    List.empty()\n    step()\n  }\n  def step(): Unit = {\n    ()\n  }\n}\n",
        )],
        expected_syms: &[
            sym("BaseService", SymKind::Interface, None, true),
            sym("MainService", SymKind::Class, None, true),
            sym("run", SymKind::Method, Some("MainService"), true),
            sym("step", SymKind::Method, Some("MainService"), true),
        ],
        expected_edges: &[("run", "step")],
        expect_external: true,
        expected_lego: Some(("BaseService", "MainService")),
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "solidity",
        files: &[(
            "contracts/Vault.sol",
            Lang::Solidity,
            b"interface IVault {}\ncontract Vault is IVault {\n    function deposit() public {\n        abi.encode();\n        recordDeposit();\n    }\n    function recordDeposit() internal {\n        return;\n    }\n}\n",
        )],
        expected_syms: &[
            sym("IVault", SymKind::Interface, None, false),
            sym("Vault", SymKind::Class, None, true),
            sym("deposit", SymKind::Function, Some("Vault"), true),
            sym("recordDeposit", SymKind::Function, Some("Vault"), true),
        ],
        expected_edges: &[("deposit", "recordDeposit")],
        expect_external: true,
        expected_lego: Some(("IVault", "Vault")),
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "sql",
        files: &[(
            "schema.sql",
            Lang::Sql,
            b"CREATE TABLE accounts ( id INT );\nCREATE VIEW active_accounts AS SELECT id FROM accounts;\n",
        )],
        expected_syms: &[
            sym("accounts", SymKind::Class, None, false),
            sym("active_accounts", SymKind::Class, None, false),
        ],
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "swift",
        files: &[(
            "src/Runner.swift",
            Lang::Swift,
            b"protocol Greeter {\n    func greet()\n}\nstruct ConsoleGreeter: Greeter {\n    func greet() {\n        DispatchQueue.main()\n        logMsg()\n    }\n    func logMsg() {\n        return\n    }\n}\n",
        )],
        expected_syms: &[
            sym("Greeter", SymKind::Interface, None, true),
            sym("ConsoleGreeter", SymKind::Class, None, true),
            sym("greet", SymKind::Function, Some("ConsoleGreeter"), true),
            sym("logMsg", SymKind::Function, Some("ConsoleGreeter"), true),
        ],
        expected_edges: &[("greet", "logMsg")],
        expect_external: true,
        expected_lego: Some(("Greeter", "ConsoleGreeter")),
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "toml",
        files: &[(
            "Cargo.toml",
            Lang::Toml,
            b"[package]\nname = \"demo\"\n[dependencies]\nserde = { version = \"1.0\" }\n",
        )],
        expected_syms: &[
            sym("package", SymKind::Section, None, false),
            sym("name", SymKind::Section, None, false),
            sym("dependencies", SymKind::Section, None, false),
            sym("serde", SymKind::Section, None, false),
        ],
        rejected_syms: &["version"],
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "typescript",
        files: &[(
            "src/service.ts",
            Lang::TypeScript,
            b"import { log } from './logger';\nexport const MAX_BATCH = 64;\nexport const camelNoise = 1;\nexport interface Handler { handle(): void; }\nexport class ApiHandler implements Handler {\n  handle(): void {\n    Array.isArray([]);\n    dispatch();\n  }\n}\nexport function dispatch(): void {\n  return;\n}\n",
        )],
        expected_syms: &[
            sym("MAX_BATCH", SymKind::Var, None, false),
            sym("Handler", SymKind::Interface, None, true),
            sym("ApiHandler", SymKind::Class, None, true),
            sym("handle", SymKind::Method, Some("ApiHandler"), true),
            sym("dispatch", SymKind::Function, None, true),
        ],
        rejected_syms: &["camelNoise"],
        expected_edges: &[("handle", "dispatch")],
        expect_external: true,
        expected_lego: Some(("Handler", "ApiHandler")),
        expected_import: Some("./logger"),
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "yaml",
        files: &[(
            "ci.yaml",
            Lang::Yaml,
            b"<<: *base\njobs:\n  test:\n    steps:\n      - run: cargo test\n",
        )],
        expected_syms: &[
            sym("jobs", SymKind::Section, None, false),
            sym("test", SymKind::Section, None, false),
        ],
        rejected_syms: &["<<", "steps", "run"],
..NO_CASE
            },
            None,
        ),
        (
            GraphProtocolCase {
        label: "zig",
        files: &[(
            "src/main.zig",
            Lang::Zig,
            b"const Engine = struct {\n    pub fn start() void {\n        std.print();\n        helper();\n    }\n};\nconst Mode = enum { fast, slow };\nconst Payload = union { raw: u32 };\npub fn helper() void {\n    return;\n}\n",
        )],
        expected_syms: &[
            sym("Engine", SymKind::Class, None, false),
            sym("Mode", SymKind::Var, None, false),
            sym("Payload", SymKind::Class, None, false),
            sym("start", SymKind::Function, Some("Engine"), true),
            sym("helper", SymKind::Function, None, true),
        ],
        expected_edges: &[("start", "helper")],
        expect_external: true,
..NO_CASE
            },
            None,
        ),
];

#[test]
fn graph_protocol_cases_hold_for_each_language() {
    for (case, entrypoint) in ROWS {
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

#[test]
fn std_names_and_extensions_resolve_for_each_language() {
    let cases: [StdCase; 3] = [
        (
            Lang::BrightScript,
            &["createobject"],
            &["rosgnode"],
            &["bs"],
        ),
        (
            Lang::Cobol,
            &["current-date"],
            &["sysout"],
            &["cbl", "cob", "cpy"],
        ),
        (
            Lang::Fortran,
            &["matmul"],
            &["iso_c_binding"],
            &["f90", "f"],
        ),
    ];
    for (lang, members, qualifiers, extensions) in cases {
        for name in members {
            assert!(lang.semantics().is_std_or_builtin_member(name), "{name}");
        }
        for name in qualifiers {
            assert!(lang.semantics().is_std_or_builtin_qualifier(name), "{name}");
        }
        for ext in extensions {
            assert_eq!(Lang::from_extension(ext), lang, "{ext}");
        }
    }
}

#[test]
fn import_targets_resolve_to_their_indexed_file() {
    let dart: HashMap<&str, u32> = HashMap::from([
        ("packages/core/lib/src/logger.dart", 10),
        ("packages/api/lib/src/db.dart", 11),
    ]);
    let python: HashMap<&str, u32> = HashMap::from([("pkg/mod_a.py", 6)]);
    let cases = [
        (
            Lang::Dart,
            "packages/api/lib/src/service.dart",
            "'package:core/src/logger.dart'",
            &dart,
            Some(10),
        ),
        (
            Lang::Dart,
            "packages/api/lib/src/service.dart",
            "'./db.dart'",
            &dart,
            Some(11),
        ),
        (
            Lang::Dart,
            "packages/api/lib/src/service.dart",
            "'dart:convert'",
            &dart,
            None,
        ),
        (Lang::Python, "pkg/sub.py", ".mod_a", &python, Some(6)),
    ];
    for (lang, importer, target, index, want) in cases {
        assert_eq!(
            lang.semantics()
                .resolve_import_target(importer, target, index),
            want,
            "{target}"
        );
    }
}
