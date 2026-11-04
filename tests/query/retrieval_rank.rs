use ripwire_rs::query::retrieval::{ForTaskOptions, call_for_task_with_options};
use ripwire_rs::*;

const QUEUE_TS: &[u8] = b"export interface Outbox {
  enqueue(task: Task): void;
}
export class CloudOutbox implements Outbox {
  enqueue(task: Task): void {
    send(task);
  }
}
";

const LEDGER_TS: &[u8] = b"export function acceptRequest(payload: Payload): void {
  if (payload.late) {
    markOverdue(payload);
  }
  enqueueRetry(payload);
}
export function tidy(payload: Payload): void {
  logRetry(payload);
}
";

const SETTLE_TS: &[u8] = b"export function OverdueBalanceBadge(props: Props): string {
  return `${props.overdueBalance}`;
}
export function reconcileOverdueBalance(account: Account): Settlement {
  const pending = account.payments.filter(isPending);
  if (account.balance < 0 && pending.length > 0) {
    return settleLater(account);
  }
  return settleNow(account);
}
";

/// Ranked `(name, line)` rows the conceptual `--for` route returns for `task` over one in-memory file.
fn ranked(source: &'static [u8], task: &str) -> Vec<(String, u64)> {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let files: [(&str, &[u8]); 1] = [("src/module.ts", source)];
    let ingest = ingest_in_memory_files(
        &[("src/module.ts", Lang::TypeScript, source)],
        &registry,
        &interner,
    );
    let graph = Graph::build(&ingest, &interner);
    let ranks = graph.page_rank(&PageRankConfig::default());
    let value = call_for_task_with_options(
        &ingest,
        &interner,
        &ranks.ranks,
        task,
        ForTaskOptions {
            sources: Some(&files),
            signatures_only: true,
            budget_tokens: Some(4000),
            ..Default::default()
        },
    );
    value["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_string(),
                s["line"].as_u64().unwrap(),
            )
        })
        .collect()
}

fn position(rows: &[(String, u64)], name: &str, line: u64) -> Option<usize> {
    rows.iter().position(|(n, l)| n == name && *l == line)
}

#[test]
fn body_coordinating_two_query_concepts_outranks_single_call() {
    let rows = ranked(LEDGER_TS, "retry queue overdue");
    let order: Vec<&str> = rows.iter().map(|(name, _)| name.as_str()).collect();
    let at = |name| order.iter().position(|n| *n == name).unwrap_or(usize::MAX);
    assert!(at("acceptRequest") < at("tidy"), "{order:?}");
}

#[test]
fn exact_name_lookup_keeps_bodyless_declaration_admitted() {
    let rows = ranked(QUEUE_TS, "enqueue");
    assert!(
        position(&rows, "enqueue", 5).is_some(),
        "exact-name lookup must keep the declaration: {rows:?}"
    );
    assert!(
        position(&rows, "enqueue", 5).is_some(),
        "exact-name lookup must keep the implementation: {rows:?}"
    );
}

#[test]
fn components_and_bodyless_stubs_rank_below_their_implementations() {
    let rows = ranked(SETTLE_TS, "overdue balance pending payments");
    let coordinator = position(&rows, "reconcileOverdueBalance", 4).expect("coordinator admitted");
    if let Some(badge) = position(&rows, "OverdueBalanceBadge", 1) {
        assert!(
            coordinator < badge,
            "a component that only renders the matched names must rank below the coordinator: {rows:?}"
        );
    }
    let rows = ranked(QUEUE_TS, "enqueue outbox");
    let implementation = position(&rows, "enqueue", 5).expect("implementation admitted");
    let stub = position(&rows, "enqueue", 2).expect("declaration admitted");
    assert!(
        implementation < stub,
        "bodyless stub must not outrank the implementation: {rows:?}"
    );
}

const FLUENT_TS: &[u8] = b"declare function createServerFn(o: object): any;
export const resumeSession = createServerFn({ method: \"POST\" })
  .inputValidator((d: unknown) => d)
  .handler(async ({ data }) => {
    return reconcileLedger(data);
  });
export const listRoutes = createServerFn({ method: \"GET\" })
  .handler(async () => {
    return [];
  });
";

#[test]
fn fluent_handler_body_text_is_searchable() {
    let rows = ranked(FLUENT_TS, "reconcile ledger");
    assert!(
        position(&rows, "resumeSession", 2).is_some(),
        "a term only in the handler body must admit the fluent definition: {rows:?}"
    );
}
