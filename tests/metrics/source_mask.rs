use ripwire_rs::Lang;
use ripwire_rs::metrics::source_mask::mask_source;

#[test]
fn test_mask_separates_code_comments_and_literals_per_line() {
    let src = "let a = \"strcpy(\"; // TODO: fix\nstrcpy(x); /* catch {} */\n";
    let m = mask_source(src, Lang::C);
    assert_eq!(m.code.len(), src.len());
    let code: Vec<&str> = m.code.lines().collect();
    let comments: Vec<&str> = m.comments.lines().collect();
    assert_eq!(code.len(), 2);
    assert!(!code[0].contains("strcpy("));
    assert!(!code[0].contains("TODO"));
    assert!(comments[0].contains("TODO: fix"));
    assert!(code[1].contains("strcpy(x)"));
    assert!(!code[1].contains("catch"));
    assert!(comments[1].contains("catch {}"));
}

/// Each case masks `source` as `lang` and hands the code and comment channels to its check.
type Check = fn(&str, &str);

#[test]
fn masks_literals_comments_and_interpolations_per_language() {
    let cases: [(Lang, &str, Check); 9] = [
        (Lang::Python, "x = 1  # FIXME: later\n", |_, comments| {
            assert!(comments.contains("FIXME: later"));
        }),
        (Lang::C, "#include <a.h>\n", |code, _| {
            assert!(code.contains("#include"));
        }),
        (
            Lang::Rust,
            "fn f<'a>(s: &'a str) -> char { 'x' } // todo!(\nlet q = '\\'';\n",
            |code, comments| {
                assert!(code.contains("fn f<'a>(s: &'a str)"));
                assert!(!code.contains("'x'"));
                assert!(!code.contains("todo!("));
                assert!(comments.contains("todo!("));
                assert!(!code.lines().nth(1).unwrap().contains('\''));
            },
        ),
        (
            Lang::Python,
            "result = total // divisor\nlabel = '''quoted ' # total\nstill total'''\n# total\n",
            |code, comments| {
                assert!(code.contains("total // divisor"));
                assert!(!code.contains("still total"));
                assert!(!comments.contains("quoted"));
                assert_eq!(comments.matches("total").count(), 1);
            },
        ),
        (
            Lang::Rust,
            "fn label<'a>() -> &'a str { r#\"quoted \" // TOKEN\"# }\nfn value() { consume(TOKEN); }\n",
            |code, comments| {
                assert!(code.contains("fn label<'a>() -> &'a str"));
                assert!(!code.contains("quoted"));
                assert!(code.contains("consume(TOKEN)"));
                assert!(!comments.contains("TOKEN"));
            },
        ),
        (
            Lang::JavaScript,
            "const label = `TOKEN ${value(TOKEN)} suffix`;\n",
            |code, _| {
                assert!(!code.contains("`TOKEN"));
                assert!(code.contains("value(TOKEN)"));
                assert!(!code.contains("suffix"));
            },
        ),
        (Lang::Json, "{\"limit\": 8}\n", |code, comments| {
            assert!(code.trim().is_empty(), "{code}");
            assert!(!comments.contains("explanation"));
        }),
        (Lang::Toml, "limit = 8 # explanation\n", |code, comments| {
            assert!(code.trim().is_empty(), "{code}");
            assert!(comments.contains("explanation"));
        }),
        (Lang::Yaml, "limit: 8 # explanation\n", |code, comments| {
            assert!(code.trim().is_empty(), "{code}");
            assert!(comments.contains("explanation"));
        }),
    ];
    for (lang, source, check) in cases {
        let masked = mask_source(source, lang);
        check(&masked.code, &masked.comments);
    }
}
