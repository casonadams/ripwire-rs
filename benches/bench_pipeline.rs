use ripwire_rs::domain::{Interner, Lang};
use ripwire_rs::graph::Graph;
use ripwire_rs::ingest::{LanguageRegistry, ingest_in_memory_files};
use ripwire_rs::rank::PageRankConfig;
use std::time::{Duration, Instant};

fn run_benchmark<F: FnMut()>(name: &str, mut f: F, warmup_iters: usize, bench_iters: usize) {
    for _ in 0..warmup_iters {
        f();
    }
    let mut times = Vec::with_capacity(bench_iters);
    for _ in 0..bench_iters {
        let t0 = Instant::now();
        f();
        times.push(t0.elapsed());
    }
    times.sort();
    let total: Duration = times.iter().sum();
    let avg = total / (bench_iters as u32);
    let min = times[0];
    let max = times[bench_iters - 1];
    let p50 = times[bench_iters / 2];
    println!(
        "{:<30} | avg: {:>9.3?} | min: {:>9.3?} | p50: {:>9.3?} | max: {:>9.3?}",
        name, avg, min, p50, max
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--list") {
        println!("bench_pipeline: benchmark");
        return;
    }

    println!(
        "========================================================================================="
    );
    println!("Cargo Benchmark: ripwire-rs in-memory pipeline");
    println!(
        "========================================================================================="
    );
    println!(
        "{:<30} | {:>14} | {:>14} | {:>14} | {:>14}",
        "Benchmark", "Average", "Min", "p50", "Max"
    );
    println!(
        "-----------------------------------------------------------------------------------------"
    );

    let registry = LanguageRegistry::new().unwrap();

    let code_rust = b"pub fn compute(a: i32, b: i32) -> i32 { helper(a) + b } fn helper(x: i32) -> i32 { x * 2 }";
    run_benchmark(
        "ingest_rust_ast_parse",
        || {
            let interner = Interner::new();
            let _ = ingest_in_memory_files(
                &[("src/lib.rs", Lang::Rust, code_rust)],
                &registry,
                &interner,
            );
        },
        50,
        200,
    );

    let code_py = b"def process_data(items):\n    return [transform(x) for x in items]\ndef transform(x):\n    return x + 1\n";
    let code_ts = b"export function renderView(ctx: Context): string { return formatHeader(ctx); } function formatHeader(c: Context) {}";
    run_benchmark(
        "ingest_polyglot_parse",
        || {
            let interner = Interner::new();
            let _ = ingest_in_memory_files(
                &[
                    ("src/lib.rs", Lang::Rust, code_rust),
                    ("app/main.py", Lang::Python, code_py),
                    ("web/index.ts", Lang::TypeScript, code_ts),
                ],
                &registry,
                &interner,
            );
        },
        50,
        200,
    );

    let interner = Interner::new();
    let ingest = ingest_in_memory_files(
        &[
            ("src/lib.rs", Lang::Rust, code_rust),
            ("app/main.py", Lang::Python, code_py),
            ("web/index.ts", Lang::TypeScript, code_ts),
        ],
        &registry,
        &interner,
    );
    let sources: std::collections::HashMap<String, Vec<u8>> = [
        ("src/lib.rs", code_rust.as_slice()),
        ("app/main.py", code_py.as_slice()),
        ("web/index.ts", code_ts.as_slice()),
    ]
    .into_iter()
    .map(|(path, src)| (path.to_string(), src.to_vec()))
    .collect();
    let graph = Graph::build(&ingest, &interner);
    let pr_cfg = PageRankConfig::default();
    run_benchmark(
        "pagerank_power_iteration",
        || {
            let _ = graph.page_rank(&pr_cfg);
        },
        100,
        500,
    );

    run_benchmark(
        "ast_clone_detection",
        || {
            let _ = ripwire_rs::clones::find_clones(
                &ingest,
                &interner,
                5,
                ripwire_rs::clones::CloneSource::Sources(&sources),
            );
        },
        50,
        200,
    );

    let pr = graph.page_rank(&pr_cfg);
    run_benchmark(
        "cache_payload_roundtrip",
        || {
            let payload = ripwire_rs::cache::CachePayload {
                validation_key: "benchmark_key".into(),
                interner_strings: interner.all_strings(),
                ingest: ingest.clone(),
                graph: graph.clone(),
                pr: pr.clone(),
            };
            let blob = ripwire_rs::cache::encode_payload(&payload).unwrap();
            let _ = ripwire_rs::cache::load_cached_payload_slice(&blob, "benchmark_key");
        },
        50,
        200,
    );

    println!(
        "========================================================================================="
    );
}
