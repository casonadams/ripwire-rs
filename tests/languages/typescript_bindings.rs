use crate::support::corpus::{Fixture, fixture as corpus_fixture};
use crate::support::symbols::sorted_names;
use ripwire_rs::{Lang, NodeId, Symbol};

/// A fluent server function whose handler reads a response body through a property call.
const SERVER_FN: &str = "import { createServerFn } from \"@tanstack/react-start\";\n\
export const resumeSession = createServerFn({ method: \"POST\" })\n\
  .inputValidator((d: unknown): { token: string } => ({ token: String(d) }))\n\
  .handler(async ({ data }: { data: { token: string } }) => {\n\
    const res = await fetch(data.token);\n\
    const json = await res.json();\n\
    return json;\n\
  });\n";

/// A same-named module variable in an unrelated script: a property call must never bind to it.
const REFRESH_SCRIPT: &str = "let json;\n\
export function refresh(raw: string): void {\n  json = JSON.parse(raw);\n}\n";

/// An imported server function called from a nested async IIFE inside a handler.
const ROUTE: &str = "import { resumeSession } from \"../lib/auth\";\n\
export function mount(token: string): void {\n\
  const handler = (msg: { token: string }) => {\n\
    void (async () => {\n\
      await resumeSession({ data: { token: msg.token } });\n\
    })();\n\
  };\n\
  handler({ token: token });\n\
}\n";

fn fixture(files: &[(&str, &str)]) -> Fixture {
    corpus_fixture(Lang::TypeScript, files)
}

impl Fixture {
    fn module_symbol(&self, name: &str) -> &Symbol {
        self.ingest
            .symbols
            .iter()
            .find(|sym| self.interner.resolve(&sym.name) == name && sym.scope.is_none())
            .unwrap_or_else(|| panic!("module symbol {name} not indexed"))
    }

    fn named(&self, ids: impl Iterator<Item = NodeId>) -> Vec<String> {
        sorted_names(&self.ingest, &self.interner, ids, |sym| sym.name)
    }

    fn callers(&self, name: &str) -> Vec<String> {
        let id = self.module_symbol(name).id;
        self.named(self.graph.callers(id).map(|(caller, _)| caller))
    }

    fn callees(&self, name: &str) -> Vec<String> {
        let id = self.module_symbol(name).id;
        self.named(self.graph.callees(id).map(|(callee, _)| callee))
    }
}

fn all_files() -> Fixture {
    fixture(&[
        ("src/lib/auth.ts", SERVER_FN),
        ("scripts/refresh-dev-token.ts", REFRESH_SCRIPT),
        ("src/routes/native-resume.ts", ROUTE),
    ])
}

#[test]
fn fluent_handler_bindings_and_property_calls() {
    let fx = all_files();
    let callers = fx.callers("resumeSession");
    assert!(
        callers
            .iter()
            .any(|name| name == "handler" || name == "mount"),
        "nested IIFE call not attributed: {callers:?}"
    );
    let fx = all_files();
    let callees = fx.callees("resumeSession");
    assert!(
        !callees.iter().any(|name| name == "json"),
        "property call res.json() bound to the unrelated `let json`: {callees:?}"
    );
    let fx = all_files();
    let json = fx.callers("json");
    assert!(
        json.is_empty(),
        "property call bound to a module variable: {json:?}"
    );
}
