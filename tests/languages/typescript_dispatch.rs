use crate::corpus_support::{Fixture, fixture as corpus_fixture};
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

    fn caller_names(&self, target: NodeId) -> Vec<String> {
        let mut names: Vec<String> = self
            .graph
            .callers(target)
            .map(|(id, _)| {
                let sym = self.ingest.sym(id).expect("caller symbol");
                self.interner.resolve(&sym.name).to_string()
            })
            .collect();
        names.sort();
        names
    }
}

fn all_files() -> Fixture {
    fixture(&[
        ("src/gcp/tasks.ts", TASKS),
        ("src/service/webhook.ts", WEBHOOK),
        ("tests/tasks.test.ts", CALLERS),
        ("src/service/worker.ts", WORKER),
    ])
}

fn lookup_files() -> Fixture {
    fixture(&[
        ("src/service/ticket.ts", TICKET),
        ("src/github/client.ts", CLIENT),
        ("src/service/key-search.ts", KEY_SEARCH),
    ])
}

#[test]
fn class_satisfying_the_interface_shape_without_declaring_it_is_reached() {
    let fx = all_files();
    let shaped = fx.caller_names(fx.method("ShapedQueue", "enqueue"));
    assert!(shaped.contains(&"acceptEvent".to_string()), "{shaped:?}");
}

#[test]
fn interface_typed_call_reaches_the_implementing_method() {
    let fx = all_files();
    let implementor = fx.method("ReviewTaskQueue", "enqueue");
    assert!(
        fx.caller_names(implementor)
            .contains(&"acceptEvent".to_string()),
        "{:?}",
        fx.caller_names(implementor)
    );
}

#[test]
fn unrelated_class_with_the_same_method_name_is_not_linked() {
    let fx = all_files();
    let unrelated = fx.method("OtherQueue", "enqueue");
    let callers = fx.caller_names(unrelated);
    assert!(!callers.contains(&"acceptEvent".to_string()), "{callers:?}");
    assert!(callers.contains(&"runOther".to_string()), "{callers:?}");
    let implementor = fx.caller_names(fx.method("ReviewTaskQueue", "enqueue"));
    assert!(
        !implementor.contains(&"runOther".to_string()),
        "{implementor:?}"
    );
}

#[test]
fn factory_with_explicit_return_annotation_reaches_the_class_method() {
    let fx = all_files();
    let callers = fx.caller_names(fx.method("ReviewTaskQueue", "enqueue"));
    assert!(callers.contains(&"viaFactory".to_string()), "{callers:?}");
    assert!(callers.contains(&"runDirect".to_string()), "{callers:?}");
}

#[test]
fn factory_without_annotation_returning_one_class_reaches_that_class_method() {
    let fx = all_files();
    let callers = fx.caller_names(fx.method("ReviewTaskQueue", "enqueue"));
    assert!(
        callers.contains(&"viaUnannotated".to_string()),
        "{callers:?}"
    );
    assert!(callers.contains(&"viaChain".to_string()), "{callers:?}");
    assert!(callers.contains(&"viaArrow".to_string()), "{callers:?}");
}

#[test]
fn factory_whose_returns_disagree_or_are_not_known_is_not_guessed() {
    let fx = all_files();
    let mut all = Vec::new();
    for owner in ["ReviewTaskQueue", "OtherQueue", "TaskEnqueuer"] {
        all.extend(fx.caller_names(fx.method(owner, "enqueue")));
    }
    for name in ["viaMixed", "viaAsync", "viaCycle", "viaGhost"] {
        assert!(!all.contains(&name.to_string()), "{name}: {all:?}");
    }
}

#[test]
fn unannotated_factory_does_not_link_an_unrelated_class_with_the_same_method() {
    let fx = all_files();
    let review = fx.caller_names(fx.method("ReviewTaskQueue", "enqueue"));
    assert!(
        !review.contains(&"viaOtherFactory".to_string()),
        "{review:?}"
    );
    let other = fx.caller_names(fx.method("OtherQueue", "enqueue"));
    assert!(other.contains(&"viaOtherFactory".to_string()), "{other:?}");
}

#[test]
fn interface_declaration_stays_a_callee_of_the_interface_call() {
    let fx = all_files();
    let declared = fx.method("TaskEnqueuer", "enqueue");
    assert!(
        fx.caller_names(declared)
            .contains(&"acceptEvent".to_string()),
        "{:?}",
        fx.caller_names(declared)
    );
}

#[test]
fn class_field_with_annotation_types_the_this_chain() {
    let fx = all_files();
    let callers = fx.caller_names(fx.method("ReviewTaskQueue", "enqueue"));
    assert!(callers.contains(&"runQueue".to_string()), "{callers:?}");
    assert!(!callers.contains(&"runOther".to_string()), "{callers:?}");
}

#[test]
fn constructor_parameter_property_types_the_this_chain() {
    let fx = all_files();
    let callers = fx.caller_names(fx.method("ReviewTaskQueue", "enqueue"));
    assert!(callers.contains(&"runTyped".to_string()), "{callers:?}");
}

#[test]
fn field_initialized_with_new_types_the_this_chain() {
    let fx = all_files();
    let other = fx.caller_names(fx.method("OtherQueue", "enqueue"));
    assert!(other.contains(&"runOther".to_string()), "{other:?}");
    assert!(!other.contains(&"runQueue".to_string()), "{other:?}");
}

#[test]
fn parameter_field_chain_types_through_a_same_file_class() {
    let fx = all_files();
    let callers = fx.caller_names(fx.method("ReviewTaskQueue", "enqueue"));
    assert!(callers.contains(&"drive".to_string()), "{callers:?}");
}

#[test]
fn function_typed_interface_property_is_a_member_for_conformance_and_dispatch() {
    let fx = lookup_files();
    let declared = fx.symbol("TicketLookup", "commitIssueKeys");
    assert_eq!(fx.interner.resolve(&declared.name), "commitIssueKeys");
    let callers = fx.caller_names(fx.method("GitHubClient", "commitIssueKeys"));
    assert!(callers.contains(&"keySearch".to_string()), "{callers:?}");
}

#[test]
fn function_typed_interface_property_does_not_reach_a_class_with_another_arity() {
    let fx = lookup_files();
    let scoped = fx.caller_names(fx.method("ScopedClient", "commitIssueKeys"));
    assert!(!scoped.contains(&"keySearch".to_string()), "{scoped:?}");
}

const GITHUB_ONLY: &str = "export class GitHubClient {\n  async commitIssueKeys(): Promise<string[]> {\n    return [];\n  }\n}\n";

const LOOKUP_WORKER: &str = "export async function runTask(): Promise<string[]> {\n\
  const session = await makeSession();\n  return session.client.commitIssueKeys();\n}\n";

const REGISTER: &str = "export function register(): { emit: (text: string) => string } {\n\
  return { emit: (text: string) => text };\n}\n";

const CHILD_USER: &str = "import { register } from \"./register\";\n\
export function drive(): string {\n  const child = makeChild();\n  return child.emit(\"x\");\n}\n";

#[test]
fn by_name_call_reaches_implementation_not_same_directory_contract() {
    let fx = fixture(&[
        ("src/service/ticket.ts", TICKET),
        ("src/service/worker.ts", LOOKUP_WORKER),
        ("src/github/client.ts", GITHUB_ONLY),
    ]);
    let implementor = fx.caller_names(fx.method("GitHubClient", "commitIssueKeys"));
    assert!(
        implementor.contains(&"runTask".to_string()),
        "{implementor:?}"
    );
    let contract = fx.caller_names(fx.method("TicketLookup", "commitIssueKeys"));
    assert!(!contract.contains(&"runTask".to_string()), "{contract:?}");
}

#[test]
fn interface_function_property_keeps_its_contract_for_typed_calls() {
    let fx = lookup_files();
    let contract = fx.caller_names(fx.method("TicketLookup", "commitIssueKeys"));
    assert!(contract.contains(&"keySearch".to_string()), "{contract:?}");
}

#[test]
fn object_type_literal_function_property_is_not_a_member() {
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
}

const STEP_HANDLE: &str = "export interface StepHandle {\n  isVisible: () => boolean;\n}\n\
export function showStep(step: StepHandle): boolean {\n  return step.isVisible();\n}\n";

const EXTERNAL_LOCATOR: &str = "export function ensureReady(page: { locator(s: string): unknown }): boolean {\n\
  const locator = page.locator(\"#ready\") as { isVisible(): boolean };\n  return locator.isVisible();\n}\n";

#[test]
fn untyped_call_named_like_a_function_property_does_not_bind_to_the_contract() {
    let fx = fixture(&[
        ("src/components/step-types.ts", STEP_HANDLE),
        ("e2e/helpers.ts", EXTERNAL_LOCATOR),
    ]);
    let contract = fx.caller_names(fx.method("StepHandle", "isVisible"));
    assert!(
        !contract.contains(&"ensureReady".to_string()),
        "{contract:?}"
    );
}

#[test]
fn typed_call_reaches_the_function_property_contract() {
    let fx = fixture(&[("src/components/step-types.ts", STEP_HANDLE)]);
    let contract = fx.caller_names(fx.method("StepHandle", "isVisible"));
    assert!(contract.contains(&"showStep".to_string()), "{contract:?}");
}

#[test]
fn calls_with_more_arguments_than_named_parameters_reach_a_rest_parameter_function() {
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
    assert_eq!(fx.caller_names(cn), ["field"]);
}
