use crate::support::nonlocal::{function, state_report as support_state_report};
use crate::support::state::assert_reads_are_not_state;
use ripwire_rs::Lang;
use ripwire_rs::domain::Binding;
use ripwire_rs::metrics::nonlocal::NonlocalFunction;

const CONFIG: &str = "src/config.ts";

fn state_report(files: &[(&str, &str)]) -> Vec<NonlocalFunction> {
    support_state_report(Lang::TypeScript, files)
}

/// Each expectation is `(function, Some((reads, writes)))`, or `None` when the function must not be state.
type Expect = (&'static str, Option<(usize, usize)>);

fn expect_rw(report: &[NonlocalFunction], expect: &[Expect]) {
    for &(name, rw) in expect {
        let got = function(report, name).map(|f| (f.reads, f.writes));
        assert_eq!(got, rw, "{name}");
    }
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
    assert_reads_are_not_state(
        Lang::TypeScript,
        &[(CONFIG, SCALARS), ("src/use.ts", READS_SCALARS)],
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
fn module_let_and_const_objects_are_state_only_where_written_or_read() {
    let report = state_report(&[(CONFIG, MODULE_LET), ("src/use.ts", WRITES_LET)]);
    expect_rw(
        &report,
        &[
            ("descend", Some((1, 1))),
            ("touchLegacy", Some((0, 2))),
            ("current", Some((1, 0))),
        ],
    );
    let descend = function(&report, "descend").unwrap();
    assert!(descend.cells.iter().all(|c| c.cell_name == "depth"));

    let report = state_report(&[(CONFIG, MUTABLE_OBJECT), ("src/use.ts", MUTATES_OBJECT)]);
    expect_rw(
        &report,
        &[
            ("raise", Some((0, 1))),
            ("tag", Some((0, 1))),
            ("readMax", Some((1, 0))),
        ],
    );
    let raise = function(&report, "raise").unwrap();
    assert_eq!(raise.cells[0].cell_name, "LIMITS");
    assert!(raise.cells[0].is_write);
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

const PARAM_USE: &str = "import { depth } from \"./config\";
export function reassigned(depth: number): number {
  depth = depth + 1;
  return depth;
}
export function mapped(): number[] {
  return [1].map(() => depth);
}
";

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
fn shadowing_and_environment_consts_are_not_module_state() {
    let report = state_report(&[(CONFIG, SHADOWED), ("src/use.ts", LOCAL_WRITES)]);
    expect_rw(&report, &[("localOnly", None), ("param", None)]);

    let report = state_report(&[
        ("scripts/refresh-dev-token.ts", DEV_TOKEN_SCRIPT),
        ("e2e/intercept.spec.ts", INTERCEPT_SPEC),
    ]);
    expect_rw(&report, &[("intercept", None), ("bare", None)]);

    let report = state_report(&[(CONFIG, ENV_CONFIG), ("src/use.ts", ENV_USE)]);
    expect_rw(
        &report,
        &[("owner", None), ("limit", None), ("bump", Some((0, 1)))],
    );

    let report = state_report(&[(CONFIG, MODULE_LET), ("src/use.ts", PARAM_USE)]);
    expect_rw(&report, &[("reassigned", None), ("mapped", Some((1, 0)))]);

    let report = state_report(&[(CONFIG, SHADOW_CONFIG), ("src/use.ts", SHADOW_USE)]);
    expect_rw(&report, &[("shadowed", None), ("peek", None)]);
    assert_eq!(
        binding_of(&[(CONFIG, SHADOW_CONFIG)], "cache"),
        Binding::Const
    );
}

fn analysis(files: &[(&str, &str)], lang: Lang) -> Vec<NonlocalFunction> {
    support_state_report(lang, files)
}

fn binding_of(files: &[(&str, &str)], name: &str) -> Binding {
    use ripwire_rs::CorpusView;
    let fx = crate::support::corpus::fixture(Lang::TypeScript, files);
    let view = (&fx.ingest, &fx.interner);
    fx.ingest
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

const LIST_LET: &str = "export let items: number[] = [];\n";

const LIST_USE: &str = "import { items } from \"./config\";
export function size(): number {
  return items.length + items.map((x) => x).length;
}
export function add(value: number): void {
  items.push(value);
}
";

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
fn constant_objects_and_containers_are_state_only_where_mutated() {
    let report = state_report(&[(CONFIG, MUTATION_CONFIG), ("src/use.ts", MUTATION_USE)]);
    expect_rw(
        &report,
        &[
            ("touch", Some((0, 1))),
            ("push", Some((0, 1))),
            ("forget", Some((0, 1))),
            ("peek", Some((2, 0))),
            ("total", None),
        ],
    );
    let push = function(&report, "push").unwrap();
    assert_eq!(push.cells[0].cell_name, "LIST");

    let report = state_report(&[(CONFIG, LIST_LET), ("src/use.ts", LIST_USE)]);
    expect_rw(&report, &[("size", Some((1, 0))), ("add", Some((0, 1)))]);

    let report = state_report(&[(CONFIG, LOWER_CONFIG)]);
    expect_rw(
        &report,
        &[
            ("remember", Some((0, 1))),
            ("register", Some((0, 1))),
            ("lookup", Some((1, 0))),
            ("readTable", None),
        ],
    );
    let remember = function(&report, "remember").unwrap();
    assert_eq!(remember.cells[0].cell_name, "cache");
}

const VIEW_JS: &str = "export let depth = 0;
export const view = () => <div>{depth}</div>;
";

#[test]
fn jsx_expression_reads_are_state() {
    let report = analysis(&[("src/view.js", VIEW_JS)], Lang::JavaScript);
    expect_rw(&report, &[("view", Some((1, 0)))]);
}
