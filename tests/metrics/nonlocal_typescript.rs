use crate::nonlocal_support::{function, state_report as support_state_report};
use ripwire_rs::domain::Binding;
use ripwire_rs::metrics::nonlocal::{NonlocalFunction, find_nonlocal_state};
use ripwire_rs::{Graph, Interner, Lang, LanguageRegistry, ingest_in_memory_files};

const CONFIG: &str = "src/config.ts";

fn state_report(files: &[(&str, &str)]) -> Vec<NonlocalFunction> {
    support_state_report(Lang::TypeScript, files)
}

const SCALARS: &str = "export const GREP_TIMEOUT_MS = 30_000;
export const LABEL = \"x\";
export const ON = true;
export const NOTHING = null;
export const NEGATIVE = -1;
";

const READS_SCALARS: &str =
    "import { GREP_TIMEOUT_MS, LABEL, ON, NOTHING, NEGATIVE } from \"./config\";
export function runSearch(): number {
  return GREP_TIMEOUT_MS + (ON ? 1 : 0) + NEGATIVE;
}
export function label(): string {
  return LABEL + String(NOTHING);
}
";

#[test]
fn reads_of_const_scalars_are_not_mutable_state() {
    let report = state_report(&[(CONFIG, SCALARS), ("src/use.ts", READS_SCALARS)]);
    assert!(
        report.is_empty(),
        "immutable scalar reads reported as state: {:?}",
        report.iter().map(|f| &f.sym_name).collect::<Vec<_>>()
    );
}

const MODULE_LET: &str = "let depth = 0;
var legacy = 1;
export let exported = \"a\";
export function current(): number {
  return depth;
}
";

const WRITES_LET: &str = "import { depth, legacy, exported } from \"./config\";
export function descend(): void {
  depth = depth + 1;
}
export function touchLegacy(): void {
  legacy++;
  exported += \"b\";
}
";

#[test]
fn module_level_let_and_var_are_mutable_state_with_writes() {
    let report = state_report(&[(CONFIG, MODULE_LET), ("src/use.ts", WRITES_LET)]);
    let descend = function(&report, "descend").expect("descend touches depth");
    assert_eq!((descend.reads, descend.writes), (1, 1));
    assert!(descend.cells.iter().all(|c| c.cell_name == "depth"));
    let touch = function(&report, "touchLegacy").expect("touchLegacy writes module vars");
    assert_eq!(touch.writes, 2);
    let current = function(&report, "current").expect("read of let is state");
    assert_eq!((current.reads, current.writes), (1, 0));
}

const MUTABLE_OBJECT: &str = "export const LIMITS = { max: 10 };
export const TAGS = [\"a\"];
";

const MUTATES_OBJECT: &str = "import { LIMITS, TAGS } from \"./config\";
export function raise(): void {
  LIMITS.max = 3;
}
export function tag(): void {
  TAGS[0] = \"b\";
}
export function readMax(): number {
  return LIMITS.max;
}
";

#[test]
fn mutation_through_const_object_reference_is_a_write() {
    let report = state_report(&[(CONFIG, MUTABLE_OBJECT), ("src/use.ts", MUTATES_OBJECT)]);
    let raise = function(&report, "raise").expect("member write through const is state");
    assert_eq!((raise.reads, raise.writes), (0, 1));
    assert_eq!(raise.cells[0].cell_name, "LIMITS");
    assert!(raise.cells[0].is_write);
    let tag = function(&report, "tag").expect("subscript write through const is state");
    assert_eq!(tag.writes, 1);
    let read = function(&report, "readMax").expect("reading const object contents is state");
    assert_eq!((read.reads, read.writes), (1, 0));
}

const SHADOWED: &str = "let count = 0;
";

const LOCAL_WRITES: &str = "import { count } from \"./config\";
export function localOnly(): number {
  let count = 0;
  count = count + 1;
  return count;
}
export function param(count: { n: number }): void {
  count.n = 2;
}
";

#[test]
fn writes_to_local_bindings_do_not_reach_module_state() {
    let report = state_report(&[(CONFIG, SHADOWED), ("src/use.ts", LOCAL_WRITES)]);
    assert!(
        function(&report, "localOnly").is_none(),
        "local let shadowing a module let is not module state"
    );
    assert!(
        function(&report, "param").is_none(),
        "parameter mutation is not module state"
    );
}

const DEV_TOKEN_SCRIPT: &str = "let json;
export function refreshToken(raw: string): string {
  json = JSON.parse(raw);
  return json.token;
}
";

const INTERCEPT_SPEC: &str =
    "export async function intercept(res: { json(): Promise<{ data: number }> }): Promise<number> {
  const json = await res.json();
  return json.data;
}
export async function bare(res: { json(): Promise<{ data: number }> }): Promise<number> {
  return (await res.json()).data;
}
";

#[test]
fn local_declaration_shadows_cross_file_global_of_same_name() {
    let report = state_report(&[
        ("scripts/refresh-dev-token.ts", DEV_TOKEN_SCRIPT),
        ("e2e/intercept.spec.ts", INTERCEPT_SPEC),
    ]);
    assert!(
        function(&report, "intercept").is_none(),
        "a local const json and a property call res.json() bind to another file's let json"
    );
    assert!(
        function(&report, "bare").is_none(),
        "a property call on an unknown receiver is not a read of a same-named variable"
    );
}

const ENV_CONFIG: &str = "export const OWNER = process.env.DEV_MEMBER_ID!;
export const BASE = `${process.env.HOST}/api`;
export const ENDPOINT = process.env.HOST + \"/graphql\";
export const FALLBACK = process.env.PORT ?? \"3000\";
export const LIMITS = { max: process.env.MAX };
export let counter = 0;
";

const ENV_USE: &str =
    "import { OWNER, BASE, ENDPOINT, FALLBACK, LIMITS, counter } from \"./config\";
export function owner(): string {
  return OWNER + BASE + ENDPOINT + FALLBACK;
}
export function limit(): unknown {
  return LIMITS.max;
}
export function bump(): void {
  counter++;
}
";

#[test]
fn const_bound_to_environment_scalar_is_not_mutable_state() {
    let report = state_report(&[(CONFIG, ENV_CONFIG), ("src/use.ts", ENV_USE)]);
    assert!(
        function(&report, "owner").is_none(),
        "env-derived scalar consts are immutable: {:?}",
        function(&report, "owner").map(|f| &f.cells)
    );
    assert!(
        function(&report, "limit").is_none(),
        "an unmutated const object is configuration, not state"
    );
    let bump = function(&report, "bump").expect("genuine let write stays state");
    assert_eq!((bump.reads, bump.writes), (0, 1));
}

fn analysis(files: &[(&str, &str)], lang: Lang) -> Vec<NonlocalFunction> {
    let interner = Interner::new();
    let registry = LanguageRegistry::new().unwrap();
    let sources: Vec<(&str, Lang, &[u8])> = files
        .iter()
        .map(|(path, src)| (*path, lang, src.as_bytes()))
        .collect();
    let ingest = ingest_in_memory_files(&sources, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    find_nonlocal_state(&ingest, &graph, &interner)
}

fn binding_of(files: &[(&str, &str)], name: &str) -> Binding {
    use ripwire_rs::CorpusView;
    let interner = Interner::new();
    let registry = LanguageRegistry::new().unwrap();
    let sources: Vec<(&str, Lang, &[u8])> = files
        .iter()
        .map(|(path, src)| (*path, Lang::TypeScript, src.as_bytes()))
        .collect();
    let ingest = ingest_in_memory_files(&sources, &registry, &interner);
    let view = (&ingest, &interner);
    ingest
        .symbols
        .iter()
        .find(|s| view.sym_name(s) == name)
        .unwrap()
        .binding
}

const CONSTANTS: &str = "export const TTL = 24 * 60 * 60 * 1000;
export const NAME = \"x\" + TTL;
export const ALIAS = TTL as const;
export const FROM_ENV = process.env.HOST ?? \"localhost\";
export const PAIR = { ttl: TTL };
export const SLOTS = [1, 2];
export const FETCHED = loadSomething();
let depth = 0;
";

#[test]
fn const_initializers_classify_by_provable_scalarity() {
    let files = [(CONFIG, CONSTANTS)];
    for name in ["TTL", "NAME", "ALIAS", "FROM_ENV"] {
        assert_eq!(
            binding_of(&files, name),
            Binding::Frozen,
            "{name} is a provable scalar"
        );
    }
    for name in ["PAIR", "SLOTS", "FETCHED"] {
        assert_eq!(
            binding_of(&files, name),
            Binding::Const,
            "{name} may hold a mutable value"
        );
    }
    assert_eq!(binding_of(&files, "depth"), Binding::Mutable);
}

const MUTATION_CONFIG: &str = "export const CACHE = { hits: 0 };
export const LIST: number[] = [];
export const ITEMS = [1, 2];
export const TTL = 24 * 60 * 60 * 1000;
";

const MUTATION_USE: &str = "import { CACHE, LIST, ITEMS, TTL } from \"./config\";
export function touch(): void {
  CACHE.hits = 1;
}
export function push(value: number): void {
  LIST.push(value);
}
export function forget(): void {
  delete CACHE.hits;
}
export function total(): number {
  return ITEMS.length + TTL;
}
export function peek(): number {
  return CACHE.hits + LIST.length;
}
";

#[test]
fn constant_objects_are_state_only_where_the_corpus_mutates_them() {
    let state = analysis(
        &[(CONFIG, MUTATION_CONFIG), ("src/use.ts", MUTATION_USE)],
        Lang::TypeScript,
    );
    let touch = function(&state, "touch").expect("property write is state");
    assert_eq!((touch.reads, touch.writes), (0, 1));
    let push = function(&state, "push").expect("in-place method call is a write");
    assert_eq!((push.reads, push.writes), (0, 1));
    assert_eq!(push.cells[0].cell_name, "LIST");
    let forget = function(&state, "forget").expect("delete is a write");
    assert_eq!((forget.reads, forget.writes), (0, 1));
    let peek = function(&state, "peek").expect("reads of mutated objects are state");
    assert_eq!((peek.reads, peek.writes), (2, 0));
    assert!(
        function(&state, "total").is_none(),
        "reads of unmutated constants are configuration"
    );
}

const LIST_LET: &str = "export let items: number[] = [];\n";

const LIST_USE: &str = "import { items } from \"./config\";
export function size(): number {
  return items.length + items.map((x) => x).length;
}
export function add(value: number): void {
  items.push(value);
}
";

#[test]
fn non_mutating_calls_on_mutable_bindings_are_reads() {
    let report = state_report(&[(CONFIG, LIST_LET), ("src/use.ts", LIST_USE)]);
    let size = function(&report, "size").expect("reads of a let are state");
    assert_eq!((size.reads, size.writes), (1, 0));
    let add = function(&report, "add").expect("push on a let mutates its contents");
    assert_eq!((add.reads, add.writes), (0, 1));
}

const PARAM_USE: &str = "import { depth } from \"./config\";
export function reassigned(depth: number): number {
  depth = depth + 1;
  return depth;
}
export function mapped(): number[] {
  return [1].map(() => depth);
}
";

#[test]
fn parameter_reassignment_is_local_and_expression_bodied_arrows_read_state() {
    let report = state_report(&[(CONFIG, MODULE_LET), ("src/use.ts", PARAM_USE)]);
    assert!(
        function(&report, "reassigned").is_none(),
        "a parameter shadowing a module let is not module state"
    );
    let mapped = function(&report, "mapped").expect("a read in an arrow body is state");
    assert_eq!((mapped.reads, mapped.writes), (1, 0));
}

const VIEW_JS: &str = "export let depth = 0;
export const view = () => <div>{depth}</div>;
";

#[test]
fn jsx_expression_reads_are_state() {
    let report = analysis(&[("src/view.js", VIEW_JS)], Lang::JavaScript);
    let view = function(&report, "view").expect("JSX interpolation reads a let");
    assert_eq!((view.reads, view.writes), (1, 0));
}

const LOWER_CONFIG: &str = "const cache = new Map<string, number>();
export const registry: Record<string, number> = {};
export const table = { a: 1 };
export function remember(k: string, v: number): void {
  cache.set(k, v);
}
export function register(k: string, v: number): void {
  registry[k] = v;
}
export function lookup(k: string): number | undefined {
  return cache.get(k);
}
export function readTable(): number {
  return table.a;
}
";

#[test]
fn lowercase_module_containers_are_state_only_where_mutated() {
    let report = state_report(&[(CONFIG, LOWER_CONFIG)]);
    let remember = function(&report, "remember").expect("Map.set on a module const is a write");
    assert_eq!((remember.reads, remember.writes), (0, 1));
    assert_eq!(remember.cells[0].cell_name, "cache");
    let register = function(&report, "register").expect("indexed write through a module const");
    assert_eq!((register.reads, register.writes), (0, 1));
    let lookup = function(&report, "lookup").expect("reads of a mutated Map are state");
    assert_eq!((lookup.reads, lookup.writes), (1, 0));
    assert!(
        function(&report, "readTable").is_none(),
        "an unmutated lowercase object is configuration"
    );
}

const SHADOW_CONFIG: &str = "const cache = new Map<string, number>();
export function peek(): number {
  return cache.size;
}
";

const SHADOW_USE: &str = "export function shadowed(): number {
  const cache = new Map<string, number>();
  cache.set(\"x\", 1);
  return cache.size;
}
";

#[test]
fn function_local_shadow_is_not_evidence_for_a_module_container() {
    let report = state_report(&[(CONFIG, SHADOW_CONFIG), ("src/use.ts", SHADOW_USE)]);
    assert!(
        function(&report, "shadowed").is_none(),
        "a local Map mutated inside a function is not the module const"
    );
    assert!(
        function(&report, "peek").is_none(),
        "no mutation of the module const exists, so its reads are configuration"
    );
    assert_eq!(
        binding_of(&[(CONFIG, SHADOW_CONFIG)], "cache"),
        Binding::Const
    );
}
