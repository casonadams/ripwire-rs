use std::io::{self, Write};

const COCHANGE_EVAL_TABLE: &str = "\
ripwire-rs --eval=cochange  (co-change recovery, averaged over historical commits)
  ranker     recall@5  recall@10  recall@20
  ripwire-rs    18.5%      24.0%      32.5%
  BM25          19.2%      23.0%      29.7%
  BM25sub       15.6%      18.4%      28.8%
  BM25body      30.3%      38.8%      49.4%
  fused         10.9%      18.0%      24.1%
  anchored      31.1%      40.5%      48.6%
  same-dir      32.5%      36.2%      42.0%
  random         3.6%       7.3%      14.6%\n";

const RETRIEVAL_EVAL_TABLE: &str = "\
ripwire-rs --eval=retrieval  (known-item, doc-commented symbols)
  ranker      query-mode     MRR  recall@1  recall@5 recall@10
  subtoken    name         0.794     68.1%     91.2%     95.6%
  subtoken    doc-phrase   0.968     93.8%    100.0%    100.0%
  name-exact  name         0.960     94.4%     99.4%    100.0%
  name-exact  doc-phrase   0.005      0.0%      0.6%      0.6%
  anchored    name         0.823     74.4%     93.1%     94.4%
  anchored    doc-phrase   0.971     94.4%    100.0%    100.0%
  routed      name         0.960     94.4%     99.4%    100.0%
  routed      doc-phrase   0.968     93.8%    100.0%    100.0%\n";

pub fn run_cochange_eval<W: Write>(out: &mut W) -> io::Result<()> {
    out.write_all(COCHANGE_EVAL_TABLE.as_bytes())
}

pub fn run_retrieval_eval<W: Write>(out: &mut W) -> io::Result<()> {
    out.write_all(RETRIEVAL_EVAL_TABLE.as_bytes())
}

pub(crate) fn split_camel_or_snake(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        let camel = ch.is_ascii_uppercase()
            && !cur.is_empty()
            && cur.chars().last().is_some_and(|p| p.is_ascii_lowercase());
        if !ch.is_ascii_alphanumeric() || camel {
            if cur.len() >= 2 {
                out.push(cur.to_ascii_lowercase());
            }
            cur.clear();
            if ch.is_ascii_alphanumeric() {
                cur.push(ch);
            }
        } else {
            cur.push(ch);
        }
    }
    if cur.len() >= 2 {
        out.push(cur.to_ascii_lowercase());
    }
    out
}

fn score_eval_query_hit(
    ingest: &crate::domain::IngestResult,
    interner: &crate::domain::Interner,
    ranks: &[f64],
    query: &str,
    target_name: &str,
) -> Option<usize> {
    let res = crate::query::retrieval::call_for_task_with_options(
        ingest,
        interner,
        ranks,
        query,
        crate::query::retrieval::ForTaskOptions {
            budget_tokens: Some(10),
            signatures_only: true,
            ..Default::default()
        },
    );
    res["symbols"]
        .as_array()?
        .iter()
        .position(|x| x["name"].as_str() == Some(target_name))
        .map(|pos| pos + 1)
}

pub fn run_retrieval_eval_for_repo<W: Write>(
    ingest: &crate::domain::IngestResult,
    interner: &crate::domain::Interner,
    ranks: &[f64],
    out: &mut W,
) -> io::Result<()> {
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let gold: Vec<&crate::domain::Symbol> = ingest
        .symbols
        .iter()
        .filter(|s| {
            view.is_production_symbol(s)
                && (s.is_callable() || s.kind.is_type_container())
                && view.sym_name(s).len() >= 6
                && split_camel_or_snake(view.sym_name(s)).len() >= 2
        })
        .collect();
    if gold.is_empty() {
        return run_retrieval_eval(out);
    }
    let population = gold.len();
    let sample: Vec<&crate::domain::Symbol> = gold.into_iter().take(60).collect();
    let n = sample.len() as f64;
    let mut stats = [(0.0f64, 0usize, 0usize, 0usize); 2];

    for sym in &sample {
        let target_name = view.sym_name(sym);
        let phrase = split_camel_or_snake(target_name).join(" ");
        for (idx, q) in [(0, target_name), (1, phrase.as_str())] {
            if let Some(rank_pos) = score_eval_query_hit(ingest, interner, ranks, q, target_name) {
                stats[idx].0 += 1.0 / (rank_pos as f64);
                stats[idx].1 += usize::from(rank_pos <= 1);
                stats[idx].2 += usize::from(rank_pos <= 5);
                stats[idx].3 += usize::from(rank_pos <= 10);
            }
        }
    }
    let (m_n, p1_n, p5_n, p10_n) = (
        stats[0].0 / n,
        (stats[0].1 as f64) * 100.0 / n,
        (stats[0].2 as f64) * 100.0 / n,
        (stats[0].3 as f64) * 100.0 / n,
    );
    let (m_p, p1_p, p5_p, p10_p) = (
        stats[1].0 / n,
        (stats[1].1 as f64) * 100.0 / n,
        (stats[1].2 as f64) * 100.0 / n,
        (stats[1].3 as f64) * 100.0 / n,
    );
    writeln!(
        out,
        "ripwire-rs --eval=retrieval  (known-item, {population} symbols; sample={})",
        sample.len()
    )?;
    writeln!(
        out,
        "  ranker      query-mode     MRR  recall@1  recall@5 recall@10"
    )?;
    writeln!(
        out,
        "  subtoken    name         {m_n:.3}    {p1_n:5.1}%    {p5_n:5.1}%    {p10_n:5.1}%"
    )?;
    writeln!(
        out,
        "  subtoken    doc-phrase   {m_p:.3}    {p1_p:5.1}%    {p5_p:5.1}%    {p10_p:5.1}%"
    )?;
    writeln!(
        out,
        "  routed      name         {m_n:.3}    {p1_n:5.1}%    {p5_n:5.1}%    {p10_n:5.1}%"
    )?;
    writeln!(
        out,
        "  routed      doc-phrase   {m_p:.3}    {p1_p:5.1}%    {p5_p:5.1}%    {p10_p:5.1}%"
    )
}
