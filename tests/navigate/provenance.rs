use ripwire_rs::Lang;
use ripwire_rs::query::navigation::NavMode;

fn nav_xml(fixtures: &[(&str, Lang, &[u8])], sym: &str, mode: NavMode) -> std::io::Result<String> {
    crate::support::nav::nav_xml(fixtures, None, sym, mode)
}

fn check_nav(
    fixtures: &[(&str, Lang, &[u8])],
    sym: &str,
    mode: NavMode,
    has: &[&str],
    lacks: &[&str],
) {
    let xml = nav_xml(fixtures, sym, mode).unwrap();
    for needle in has {
        assert!(xml.contains(needle), "{sym}: missing {needle} in {xml}");
    }
    for needle in lacks {
        assert!(!xml.contains(needle), "{sym}: unexpected {needle} in {xml}");
    }
}

const AMBIGUOUS: &[(&str, Lang, &[u8])] = &[
    (
        "a/alpha.dart",
        Lang::Dart,
        b"class Alpha { void register(String n) {} }\n",
    ),
    (
        "b/beta.dart",
        Lang::Dart,
        b"class Beta { void register(String n) {} }\n",
    ),
    (
        "c/setup.dart",
        Lang::Dart,
        b"void setup(dynamic c, String n) {\n  c.register(n);\n}\n",
    ),
];

#[test]
fn callers_disclose_declined_call_sites() {
    let xml = nav_xml(AMBIGUOUS, "register", NavMode::Callers).unwrap();
    assert!(xml.contains("declined_calls=\"1\""), "{xml}");
    assert!(
        xml.contains(
            "<dc in=\"setup\" p=\"c/setup.dart:2\" cands=\"2\" via=\"declined\" of_def=\"1~d 2~d\"/>"
        ),
        "{xml}"
    );
    assert!(
        xml.contains("<def ix=\"1\" t=\"method\" sc=\"Alpha\" p=\"a/alpha.dart:1\"/>"),
        "{xml}"
    );
    assert!(xml.contains("<def ix=\"2\""), "{xml}");
}

#[test]
fn callees_disclose_declined_call_sites() {
    let xml = nav_xml(AMBIGUOUS, "setup", NavMode::Callees).unwrap();
    assert!(xml.contains("declined_calls=\"1\""), "{xml}");
    assert!(
        xml.contains("<dc n=\"register\" l=\"2\" cands=\"2\" via=\"declined\"/>"),
        "{xml}"
    );
}

#[test]
fn uses_of_undefined_symbol_is_external_not_an_error() {
    let fixtures: &[(&str, Lang, &[u8])] =
        &[("m.dart", Lang::Dart, b"void m() { final x = Widget(1); }\n")];
    let xml = nav_xml(fixtures, "Widget", NavMode::Uses).unwrap();
    assert!(
        xml.contains("defs=\"0\" external=\"1\" count=\"1\""),
        "{xml}"
    );
    assert!(nav_xml(fixtures, "Unreferenced", NavMode::Uses).is_err());
}

#[test]
fn callers_and_callees_label_each_row_with_its_binding() {
    type Case<'a> = (
        &'a [(&'a str, Lang, &'a [u8])],
        &'a str,
        NavMode,
        &'a [&'a str],
        &'a [&'a str],
    );
    let cases: [Case; 4] = [
        (
            &[
                ("a/alpha.dart", Lang::Dart, b"class Alpha { void register(String n) {} }\n"),
                ("b/beta.dart", Lang::Dart, b"class Beta { void register(String n) {} }\n"),
                ("b/use.dart", Lang::Dart, b"void viaBeta(Beta k) {\n  Beta.register('x');\n}\n"),
            ],
            "register",
            NavMode::Callers,
            &["n=\"viaBeta\"", "of_def=\"2\""],
            &[],
        ),
        (
            &[
                ("a/x.dart", Lang::Dart, b"class X { static List<String> cols() => []; }\n"),
                ("b/y.dart", Lang::Dart, b"class Y { static List<String> cols() => []; }\n"),
                ("c/load.dart", Lang::Dart, b"void load() {\n  final q = \"SELECT ${pkg.Widget.cols().join(', ')}\";\n}\n"),
            ],
            "load",
            NavMode::Callees,
            &[],
            &["declined_calls=\"", "n=\"cols\""],
        ),
        (
            &[
                ("a/one.py", Lang::Python, b"def run():\n    return 1\n\ndef same_file():\n    return run()\n"),
                ("a/three.py", Lang::Python, b"def same_dir():\n    return run()\n"),
                ("b/two.py", Lang::Python, b"def run():\n    return 2\n"),
            ],
            "run",
            NavMode::Callers,
            &[
                "<s t=\"fn\" n=\"same_file\" p=\"a/one.py:4\" of_def=\"1\"",
                "<s t=\"fn\" n=\"same_dir\" p=\"a/three.py:1\" via=\"name\" of_def=\"1~n\"",
            ],
            &[],
        ),
        (
            &[
                ("b/k.py", Lang::Python, b"class K:\n    def go(self):\n        return 1\n\n    def me(self):\n        return self.go()\n"),
                ("b/l.py", Lang::Python, b"class L:\n    def go(self):\n        return 2\n"),
            ],
            "me",
            NavMode::Callees,
            &["<s t=\"fn\" n=\"go\" p=\"b/k.py:2\"/>"],
            &[],
        ),
    ];
    for (fixtures, sym, mode, has, lacks) in cases {
        check_nav(fixtures, sym, mode, has, lacks);
    }
}

#[test]
fn ambiguous_call_arms_are_labelled_split_and_same_language_wins() {
    let fixtures: &[(&str, Lang, &[u8])] = &[
        ("a/x.dart", Lang::Dart, b"class X { void run() {} }\n"),
        ("a/y.dart", Lang::Dart, b"class Y { void run() {} }\n"),
        (
            "a/use.dart",
            Lang::Dart,
            b"void go(dynamic c) {\n  c.run();\n}\n",
        ),
    ];
    let callees = nav_xml(fixtures, "go", NavMode::Callees).unwrap();
    assert_eq!(callees.matches("prov=\"split\"").count(), 2, "{callees}");
    let callers = nav_xml(fixtures, "run", NavMode::Callers).unwrap();
    assert!(callers.contains("of_def=\"1~s 2~s\""), "{callers}");

    let fixtures: &[(&str, Lang, &[u8])] = &[
        ("bind.cpp", Lang::Cpp, PYBIND),
        (
            "use.py",
            Lang::Python,
            b"def add(a, b):\n    return a\n\ndef local_call():\n    return add(1, 2)\n",
        ),
        (
            "plain.cpp",
            Lang::Cpp,
            b"int helper_impl() { return 1; }\nvoid reg(Registry& r) { r.def(\"helper\", &helper_impl); }\n",
        ),
        ("use2.py", Lang::Python, b"def other():\n    return lib.helper()\n"),
    ];
    let local = nav_xml(fixtures, "local_call", NavMode::Callees).unwrap();
    assert!(local.contains("n=\"add\" p=\"use.py:1\""), "{local}");
    assert!(!local.contains("prov=\"binding\""), "{local}");
    let other = nav_xml(fixtures, "other", NavMode::Callees).unwrap();
    assert!(other.contains("count=\"0\""), "{other}");
}

const PYBIND: &[u8] = b"#include <pybind11/pybind11.h>
namespace py = pybind11;
int add_impl(int a, int b) { return a + b; }
int add(int a, int b) { return a - b; }
int plain(int a) { return a; }
struct Pet { void bark() {} };
PYBIND11_MODULE(native, m) {
    m.def(\"add\", &add_impl);
    m.def(\"plain_fn\", plain);
    py::class_<Pet>(m, \"Pet\").def(py::init<>()).def(\"speak\", &Pet::bark);
}
";

#[test]
fn callees_preserve_the_definition_group_owning_each_callee() {
    let fixtures: &[(&str, Lang, &[u8])] = &[
        (
            "a/core.py",
            Lang::Python,
            b"def process():\n    return alpha()\n\ndef alpha():\n    return 1\n",
        ),
        (
            "b/core.py",
            Lang::Python,
            b"def process():\n    return beta()\n\ndef beta():\n    return 2\n",
        ),
    ];
    let xml = nav_xml(fixtures, "process", NavMode::Callees).unwrap();
    assert!(xml.contains("defs=\"2\""), "{xml}");
    assert!(
        xml.contains("<def ix=\"1\" t=\"fn\" p=\"a/core.py:1\"/>"),
        "{xml}"
    );
    assert!(
        xml.contains("<def ix=\"2\" t=\"fn\" p=\"b/core.py:1\"/>"),
        "{xml}"
    );
    for (name, group) in [("alpha", "1"), ("beta", "2")] {
        let prefix = format!("<s t=\"fn\" n=\"{name}\" ");
        let row = xml
            .split_once(&prefix)
            .unwrap_or_else(|| panic!("missing {name}: {xml}"))
            .1
            .split_once("/>")
            .unwrap()
            .0;
        assert!(row.contains(&format!("of_def=\"{group}\"")), "{xml}");
    }
}

#[test]
fn callers_keep_the_uncapped_default_neighborhood() {
    let mut source = String::from("def target():\n    return 0\n");
    for ix in 0..45 {
        source.push_str(&format!("\ndef caller_{ix}():\n    return target()\n"));
    }
    let fixtures = [("many.py", Lang::Python, source.as_bytes())];
    let xml = nav_xml(&fixtures, "target", NavMode::Callers).unwrap();
    assert!(xml.contains("count=\"45\""), "{xml}");
    assert_eq!(xml.matches("<s ").count(), 45, "{xml}");
}

#[test]
fn template_and_macro_rows_keep_their_binding_markers() {
    let template: &[(&str, Lang, &[u8])] = &[(
        "box.cpp",
        Lang::Cpp,
        b"template <typename T> struct Box { T get() { return T(); } };\ntemplate <> struct Box<int> { int get() { return 7; } };\nint use_box() { Box<int> b; return b.get(); }\n",
    )];
    let mac: &[(&str, Lang, &[u8])] = &[(
        "mac.c",
        Lang::C,
        b"#define SQ(x) ((x)*(x))\nint use_sq() { return SQ(3); }\n",
    )];
    let cases = [
        (
            template,
            "get",
            NavMode::Callers,
            "<def ix=\"2\" t=\"method\" sc=\"Box&lt;int&gt;\"",
        ),
        (
            template,
            "get",
            NavMode::Callers,
            "n=\"use_box\" p=\"box.cpp:3\" of_def=\"2~t\"",
        ),
        (
            mac,
            "use_sq",
            NavMode::Callees,
            "<s t=\"macro\" n=\"SQ\" p=\"mac.c:1\" role=\"macro\"/>",
        ),
    ];
    for (fixtures, symbol, mode, needle) in cases {
        let xml = nav_xml(fixtures, symbol, mode).unwrap();
        assert!(xml.contains(needle), "{xml}");
    }
}

#[test]
fn bound_edges_carry_their_provenance_in_each_language() {
    {
        let fixtures: &[(&str, Lang, &[u8])] = &[
            (
                "pkg/util.py",
                Lang::Python,
                b"def helper():\n    return 1\n",
            ),
            (
                "other/util.py",
                Lang::Python,
                b"def helper():\n    return 2\n",
            ),
            (
                "app.py",
                Lang::Python,
                b"import pkg.util as u\n\ndef main():\n    return u.helper() + solo()\n",
            ),
            ("lib/solo.py", Lang::Python, b"def solo():\n    return 3\n"),
            (
                "a/x.dart",
                Lang::Dart,
                b"class X { static int cols() => 1; }\n",
            ),
            (
                "a/y.dart",
                Lang::Dart,
                b"class Y { static int cols() => 2; }\n",
            ),
            (
                "b/load.dart",
                Lang::Dart,
                b"int load() {\n  return X.cols();\n}\n",
            ),
        ];
        check_nav(
            fixtures,
            "main",
            NavMode::Callees,
            &[
                "n=\"helper\" p=\"pkg/util.py:1\" prov=\"import\"/>",
                "n=\"solo\" p=\"lib/solo.py:1\" via=\"name\"/>",
            ],
            &[],
        );
        check_nav(
            fixtures,
            "load",
            NavMode::Callees,
            &["n=\"cols\" p=\"a/x.dart:1\" prov=\"final-segment\"/>"],
            &[],
        );
    }
    {
        let fixtures: &[(&str, Lang, &[u8])] = &[
        ("inc/shape.h", Lang::C, b"int area(int w, int h);\n"),
        ("inc/old.h", Lang::C, b"int area(int w);\n"),
        (
            "src/shape.c",
            Lang::C,
            b"int area(int w, int h) { return w * h; }\nint total() { return area(1, 2); }\n",
        ),
        (
            "src/ov.ts",
            Lang::TypeScript,
            b"export function fmt(x: string): string;\nexport function fmt(x: number): string;\nexport function fmt(x: any): string { return String(x); }\nexport function callFmt() { return fmt(1); }\n",
        ),
    ];
        let defs = "<def ix=\"1\" t=\"fn\" p=\"inc/old.h:1\" bodyless=\"1\"/><def ix=\"2\" t=\"fn\" p=\"src/shape.c:1\"/>";
        check_nav(
            fixtures,
            "area",
            NavMode::Callers,
            &[
                "defs=\"3\"",
                defs,
                "n=\"total\" p=\"src/shape.c:2\" of_def=\"2\"",
            ],
            &["inc/shape.h"],
        );
        check_nav(
            fixtures,
            "callFmt",
            NavMode::Callees,
            &["<s t=\"fn\" n=\"fmt\" p=\"src/ov.ts:3\"/>"],
            &["prov=\"split\""],
        );
        check_nav(fixtures, "area", NavMode::Callees, &[defs], &[]);
    }
    {
        let fixtures: &[(&str, Lang, &[u8])] = &[
        ("bind.cpp", Lang::Cpp, PYBIND),
        (
            "use.py",
            Lang::Python,
            b"import native\n\ndef free_call():\n    return native.add(1, 2)\n\ndef method_call(p):\n    return p.speak()\n\ndef unexported():\n    return native.plain_fn(1)\n",
        ),
    ];
        check_nav(
            fixtures,
            "free_call",
            NavMode::Callees,
            &["<s t=\"fn\" n=\"add_impl\" p=\"bind.cpp:3\" prov=\"binding\"/>"],
            &["n=\"add\" "],
        );
        check_nav(
            fixtures,
            "method_call",
            NavMode::Callees,
            &["n=\"bark\"", "prov=\"binding\""],
            &[],
        );
        check_nav(
            fixtures,
            "unexported",
            NavMode::Callees,
            &["count=\"0\""],
            &[],
        );
    }
}
