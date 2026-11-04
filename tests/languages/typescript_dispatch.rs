use crate::support::corpus::{Fixture, fixture as corpus_fixture};
use crate::support::symbols::sorted_names;
use ripwire_rs::{Lang, NodeId, Symbol};

const TASKS: &str = "export interface Task {\n  id: string;\n}\n\
export interface TaskEnqueuer {\n  enqueue(payload: Task): Promise<string>;\n  close(): void;\n}\n\
export class ReviewTaskQueue implements TaskEnqueuer {\n  async enqueue(payload: Task): Promise<string> {\n    return payload.id;\n  }\n  close(): void {}\n}\n\
export class ShapedQueue {\n  async enqueue(payload: Task): Promise<string> {\n    return payload.id;\n  }\n  close(): void {}\n}\n\
export class OtherQueue {\n  enqueue(payload: Task): number {\n    return payload.id.length;\n  }\n}\n";

const WEBHOOK: &str = "import type { Task, TaskEnqueuer } from \"../gcp/tasks\";\n\
export function acceptEvent(input: { queue: TaskEnqueuer }, task: Task): Promise<string> {\n  return input.queue.enqueue(task);\n}\n";

const CALLERS: &str = "import { ReviewTaskQueue, OtherQueue } from \"../gcp/tasks\";\n\
export function runDirect(): void {\n  const queue = new ReviewTaskQueue();\n  queue.enqueue({ id: \"a\" });\n}\n\
export function runOther(): void {\n  const other = new OtherQueue();\n  other.enqueue({ id: \"b\" });\n}\n\
export function makeQueue(): ReviewTaskQueue {\n  return new ReviewTaskQueue();\n}\n\
export function viaFactory(): void {\n  makeQueue().enqueue({ id: \"c\" });\n}\n\
export function viaUnannotated(): void {\n  unannotated().enqueue({ id: \"d\" });\n}\n\
function unannotated() {\n  return new ReviewTaskQueue();\n}\n\
export function viaChain(): void {\n  chained().enqueue({ id: \"e\" });\n}\n\
function chained() {\n  return unannotated();\n}\n\
export function viaMixed(flag: boolean): void {\n  mixed(flag).enqueue({ id: \"f\" });\n}\n\
function mixed(flag: boolean) {\n  if (flag) {\n    return new ReviewTaskQueue();\n  }\n  return new OtherQueue();\n}\n\
export function viaOtherFactory(): void {\n  otherFactory().enqueue({ id: \"g\" });\n}\n\
function otherFactory() {\n  return new OtherQueue();\n}\n\
export function viaAsync(): void {\n  asyncFactory().enqueue({ id: \"h\" });\n}\n\
async function asyncFactory() {\n  return new ReviewTaskQueue();\n}\n\
export function viaCycle(): void {\n  cycleA().enqueue({ id: \"i\" });\n}\n\
function cycleA() {\n  return cycleB();\n}\n\
function cycleB() {\n  return cycleA();\n}\n\
export function viaGhost(): void {\n  ghost().enqueue({ id: \"j\" });\n}\n\
function ghost() {\n  return new Ghost();\n}\n\
export const arrowFactory = () => new ReviewTaskQueue();\n\
export function viaArrow(): void {\n  arrowFactory().enqueue({ id: \"k\" });\n}\n";

const WORKER: &str = "import { OtherQueue, ReviewTaskQueue } from \"../gcp/tasks\";\n\
export class Worker {\n  private queue: ReviewTaskQueue;\n  private other = new OtherQueue();\n\
  constructor(private readonly typed: ReviewTaskQueue) {}\n\
  runQueue(): void {\n    this.queue.enqueue({ id: \"a\" });\n  }\n\
  runTyped(): void {\n    this.typed.enqueue({ id: \"b\" });\n  }\n\
  runOther(): void {\n    this.other.enqueue({ id: \"c\" });\n  }\n\
}\n\
export function drive(w: Worker): void {\n  w.queue.enqueue({ id: \"d\" });\n}\n";

const TICKET: &str =
    "export interface TicketLookup {\n  commitIssueKeys: () => Promise<string[]>;\n}\n";

const CLIENT: &str = "export class GitHubClient {\n  async commitIssueKeys(): Promise<string[]> {\n    return [];\n  }\n}\n\
export class ScopedClient {\n  async commitIssueKeys(scope: string): Promise<string[]> {\n    return [scope];\n  }\n}\n";

const KEY_SEARCH: &str = "import type { TicketLookup } from \"../service/ticket\";\n\
export async function keySearch(lookup: TicketLookup): Promise<string[]> {\n  return lookup.commitIssueKeys();\n}\n";

const GITHUB_ONLY: &str = "export class GitHubClient {\n  async commitIssueKeys(): Promise<string[]> {\n    return [];\n  }\n}\n";

const LOOKUP_WORKER: &str = "export async function runTask(): Promise<string[]> {\n\
  const session = await makeSession();\n  return session.client.commitIssueKeys();\n}\n";

const REGISTER: &str = "export function register(): { emit: (text: string) => string } {\n\
  return { emit: (text: string) => text };\n}\n";

const CHILD_USER: &str = "import { register } from \"./register\";\n\
export function drive(): string {\n  const child = makeChild();\n  return child.emit(\"x\");\n}\n";

const STEP_HANDLE: &str = "export interface StepHandle {\n  isVisible: () => boolean;\n}\n\
export function showStep(step: StepHandle): boolean {\n  return step.isVisible();\n}\n";

const EXTERNAL_LOCATOR: &str = "export function ensureReady(page: { locator(s: string): unknown }): boolean {\n\
  const locator = page.locator(\"#ready\") as { isVisible(): boolean };\n  return locator.isVisible();\n}\n";

const ALL_FILES: &[(&str, &str)] = &[
    ("src/gcp/tasks.ts", TASKS),
    ("src/service/webhook.ts", WEBHOOK),
    ("tests/tasks.test.ts", CALLERS),
    ("src/service/worker.ts", WORKER),
];

const LOOKUP_FILES: &[(&str, &str)] = &[
    ("src/service/ticket.ts", TICKET),
    ("src/github/client.ts", CLIENT),
    ("src/service/key-search.ts", KEY_SEARCH),
];

const BY_NAME_FILES: &[(&str, &str)] = &[
    ("src/service/ticket.ts", TICKET),
    ("src/service/worker.ts", LOOKUP_WORKER),
    ("src/github/client.ts", GITHUB_ONLY),
];

const UNTYPED_STEP_FILES: &[(&str, &str)] = &[
    ("src/components/step-types.ts", STEP_HANDLE),
    ("e2e/helpers.ts", EXTERNAL_LOCATOR),
];

const TYPED_STEP_FILES: &[(&str, &str)] = &[("src/components/step-types.ts", STEP_HANDLE)];

/// Callers of `owner.name` in `files` must include `includes` and must not include `excludes`.
struct Reach {
    files: &'static [(&'static str, &'static str)],
    owner: &'static str,
    name: &'static str,
    includes: &'static [&'static str],
    excludes: &'static [&'static str],
}

const REACH_ROWS: &[Reach] = &[
    Reach {
        files: ALL_FILES,
        owner: "ShapedQueue",
        name: "enqueue",
        includes: &["acceptEvent"],
        excludes: &[],
    },
    Reach {
        files: ALL_FILES,
        owner: "ReviewTaskQueue",
        name: "enqueue",
        includes: &["acceptEvent"],
        excludes: &[],
    },
    Reach {
        files: ALL_FILES,
        owner: "ReviewTaskQueue",
        name: "enqueue",
        includes: &["runDirect", "viaFactory"],
        excludes: &[],
    },
    Reach {
        files: ALL_FILES,
        owner: "ReviewTaskQueue",
        name: "enqueue",
        includes: &["viaUnannotated", "viaChain", "viaArrow"],
        excludes: &[],
    },
    Reach {
        files: ALL_FILES,
        owner: "ReviewTaskQueue",
        name: "enqueue",
        includes: &[],
        excludes: &["runOther"],
    },
    Reach {
        files: ALL_FILES,
        owner: "ReviewTaskQueue",
        name: "enqueue",
        includes: &["runQueue"],
        excludes: &["runOther"],
    },
    Reach {
        files: ALL_FILES,
        owner: "ReviewTaskQueue",
        name: "enqueue",
        includes: &["runTyped"],
        excludes: &[],
    },
    Reach {
        files: ALL_FILES,
        owner: "ReviewTaskQueue",
        name: "enqueue",
        includes: &["drive"],
        excludes: &[],
    },
    Reach {
        files: ALL_FILES,
        owner: "ReviewTaskQueue",
        name: "enqueue",
        includes: &[],
        excludes: &["viaOtherFactory"],
    },
    Reach {
        files: ALL_FILES,
        owner: "OtherQueue",
        name: "enqueue",
        includes: &["viaOtherFactory", "runOther"],
        excludes: &["acceptEvent"],
    },
    Reach {
        files: ALL_FILES,
        owner: "OtherQueue",
        name: "enqueue",
        includes: &["runOther"],
        excludes: &["runQueue"],
    },
    Reach {
        files: ALL_FILES,
        owner: "ReviewTaskQueue",
        name: "enqueue",
        includes: &[],
        excludes: &["viaMixed", "viaAsync", "viaCycle", "viaGhost"],
    },
    Reach {
        files: ALL_FILES,
        owner: "OtherQueue",
        name: "enqueue",
        includes: &[],
        excludes: &["viaMixed", "viaAsync", "viaCycle", "viaGhost"],
    },
    Reach {
        files: ALL_FILES,
        owner: "TaskEnqueuer",
        name: "enqueue",
        includes: &["acceptEvent"],
        excludes: &["viaMixed", "viaAsync", "viaCycle", "viaGhost"],
    },
    Reach {
        files: LOOKUP_FILES,
        owner: "GitHubClient",
        name: "commitIssueKeys",
        includes: &["keySearch"],
        excludes: &[],
    },
    Reach {
        files: LOOKUP_FILES,
        owner: "ScopedClient",
        name: "commitIssueKeys",
        includes: &[],
        excludes: &["keySearch"],
    },
    Reach {
        files: LOOKUP_FILES,
        owner: "TicketLookup",
        name: "commitIssueKeys",
        includes: &["keySearch"],
        excludes: &[],
    },
    Reach {
        files: BY_NAME_FILES,
        owner: "GitHubClient",
        name: "commitIssueKeys",
        includes: &["runTask"],
        excludes: &[],
    },
    Reach {
        files: BY_NAME_FILES,
        owner: "TicketLookup",
        name: "commitIssueKeys",
        includes: &[],
        excludes: &["runTask"],
    },
    Reach {
        files: UNTYPED_STEP_FILES,
        owner: "StepHandle",
        name: "isVisible",
        includes: &[],
        excludes: &["ensureReady"],
    },
    Reach {
        files: TYPED_STEP_FILES,
        owner: "StepHandle",
        name: "isVisible",
        includes: &["showStep"],
        excludes: &[],
    },
];

fn fixture(files: &[(&str, &str)]) -> Fixture {
    corpus_fixture(Lang::TypeScript, files)
}

impl Fixture {
    /// The method `name` declared in class or interface `owner`.
    fn method(&self, owner: &str, name: &str) -> NodeId {
        self.symbol(owner, name).id
    }

    fn symbol(&self, owner: &str, name: &str) -> &Symbol {
        self.ingest
            .symbols
            .iter()
            .find(|sym| {
                self.interner.resolve(&sym.name) == name
                    && sym
                        .scope
                        .is_some_and(|scope| self.interner.resolve(&scope) == owner)
            })
            .unwrap_or_else(|| panic!("{owner}.{name} not indexed"))
    }
}

#[test]
fn dispatch_reaches_declared_owners_and_contracts_only() {
    for row in REACH_ROWS {
        let fx = fixture(row.files);
        let target = fx.method(row.owner, row.name);
        let callers = sorted_names(
            &fx.ingest,
            &fx.interner,
            fx.graph.callers(target).map(|(id, _)| id),
            |sym| sym.name,
        );
        for caller in row.includes {
            assert!(
                callers.contains(&caller.to_string()),
                "{}.{}: {caller} missing from {callers:?}",
                row.owner,
                row.name
            );
        }
        for caller in row.excludes {
            assert!(
                !callers.contains(&caller.to_string()),
                "{}.{}: {caller} present in {callers:?}",
                row.owner,
                row.name
            );
        }
    }
}

#[test]
fn function_typed_interface_property_is_a_member_for_conformance_and_dispatch() {
    let fx = fixture(LOOKUP_FILES);
    let declared = fx.symbol("TicketLookup", "commitIssueKeys");
    assert_eq!(fx.interner.resolve(&declared.name), "commitIssueKeys");
}

#[test]
fn object_type_literal_is_not_a_member_and_rest_params_bind() {
    let fx = fixture(&[
        ("tests/pi/register.ts", REGISTER),
        ("tests/pi/child.ts", CHILD_USER),
    ]);
    let members: Vec<&Symbol> = fx
        .ingest
        .symbols
        .iter()
        .filter(|sym| fx.interner.resolve(&sym.name) == "emit")
        .collect();
    assert!(members.is_empty(), "{members:?}");
    let fx = fixture(&[
        (
            "src/lib/utils.ts",
            "export function cn(...inputs: string[]): string {\n  return inputs.join(\" \");\n}\n",
        ),
        (
            "src/components/field.tsx",
            "import { cn } from \"@/lib/utils\";\nexport function field(): string {\n  return cn(\"a\", \"b\", \"c\");\n}\n",
        ),
    ]);
    let cn = fx
        .ingest
        .symbols
        .iter()
        .find(|sym| fx.interner.resolve(&sym.name) == "cn")
        .expect("cn indexed")
        .id;
    let callers = sorted_names(
        &fx.ingest,
        &fx.interner,
        fx.graph.callers(cn).map(|(id, _)| id),
        |sym| sym.name,
    );
    assert_eq!(callers, ["field"]);
}
