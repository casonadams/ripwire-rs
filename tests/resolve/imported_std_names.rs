use crate::support::callees::callees_described;
use ripwire_rs::{Interner, Lang};

/// A callee described by its simple name.
fn callee_name(ingest: &ripwire_rs::IngestResult, interner: &Interner, id: u32) -> String {
    interner
        .resolve(&ingest.symbols[id as usize].name)
        .to_owned()
}

const DEFINER: &[u8] = b"export function reject(code: string) { return code; }\n";

type Case<'a> = (&'a str, &'a [(&'a str, Lang, &'a [u8])], &'a [&'a str]);

#[test]
fn std_named_functions_bind_only_when_imported_or_by_a_real_call() {
    let cases: [Case; 3] = [
        (
            "check",
            &[
                ("src/errors.ts", Lang::TypeScript, DEFINER),
                (
                    "src/check.ts",
                    Lang::TypeScript,
                    b"import { reject } from \"./errors\";\nexport function check() { reject(\"bad\"); }\n",
                ),
            ],
            &["reject"],
        ),
        (
            "check",
            &[
                ("src/errors.ts", Lang::TypeScript, DEFINER),
                (
                    "src/other.ts",
                    Lang::TypeScript,
                    b"export function check() { reject(\"bad\"); }\n",
                ),
            ],
            &[],
        ),
        (
            "probe",
            &[
                ("src/query/mod.rs", Lang::Rust, b"pub mod map;\npub mod json;\n"),
                (
                    "src/app.rs",
                    Lang::Rust,
                    b"fn probe(v: Vec<u8>) { let _ = v.iter().map(|x| x + 1).count(); }\n",
                ),
            ],
            &[],
        ),
    ];
    for (caller, files, expected) in cases {
        assert_eq!(
            callees_described(files, caller, callee_name),
            expected,
            "{caller} over {files:?}"
        );
    }
}
