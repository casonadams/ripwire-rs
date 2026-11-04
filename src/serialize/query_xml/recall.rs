use crate::serialize::{CountingWriter, JsonValueExt};
use serde_json::Value;
use std::io::{self, Write};

fn write_recall_sections_note<W: Write>(
    out: &mut W,
    s: &Value,
    whole_bytes: u64,
) -> io::Result<()> {
    let selected = s.u64_or("selected", 0);
    let total = s.u64_or("total", 0);
    let lines = s.str_or("lines", "");
    let dropped = s.u64_or("dropped_by_budget", 0);
    if dropped == 0 {
        write!(
            out,
            "  [sections: {selected} of {total}, section-granular; whole doc {whole_bytes} B; lines=\"{lines}\"]"
        )
    } else {
        let matched = s.u64_or("matched", 0);
        write!(
            out,
            "  [sections: {selected} of {matched} selected ({total} in doc), section-granular; whole doc {whole_bytes} B; lines=\"{lines}\"; dropped_by_budget={dropped}]"
        )
    }
}

fn write_recall_doc<W: Write>(out: &mut W, d: &Value) -> io::Result<()> {
    write!(
        out,
        "━━ {}  (relevance {:.3}) ━━",
        d.str_or("path", ""),
        d.f64_any(&["score"], 0.0)
    )?;
    if d.bool_or("generated", false) {
        out.write_all(b"  [generated_demoted: marker]")?;
    }
    if let Some(s) = d.get("sections") {
        write_recall_sections_note(out, s, d.u64_or("whole_bytes", 0))?;
    }
    if let Some(t) = d.get("truncated") {
        write!(
            out,
            "  [truncated: {} of {} bytes]",
            t.u64_or("shown_bytes", 0),
            t.u64_or("of_bytes", 0)
        )?;
    }
    out.write_all(b"\n")?;
    out.write_all(d.str_or("content", "").as_bytes())?;
    out.write_all(b"\n\n")
}

fn write_recall_footer<W: Write>(out: &mut W, val: &Value) -> io::Result<()> {
    let relevant = val.u64_or("relevant", 0);
    if relevant == 0 {
        return out.write_all("(no relevant documents — try different terms)\n".as_bytes());
    }
    let by_k = val.u64_or("omitted_top_k", 0);
    let by_budget = val.u64_or("omitted_budget", 0);
    if by_k == 0 && by_budget == 0 {
        return Ok(());
    }
    write!(
        out,
        "(capped: {} of {relevant} relevant document files omitted — ",
        by_k + by_budget
    )?;
    if by_k > 0 {
        write!(out, "raise --top-k/top_k for {by_k} more")?;
    }
    if by_budget > 0 {
        if by_k > 0 {
            out.write_all(b"; ")?;
        }
        let budget = val.u64_or("budget_bytes", 0);
        write!(
            out,
            "raise --max-tokens/budget_tokens or narrow the query for {by_budget} more (~{budget}-byte budget)"
        )?;
    }
    out.write_all(b")\n")
}

fn write_recall_header<W: Write>(
    out: &mut W,
    query: &str,
    val: &Value,
    shown: usize,
    est_tokens: u64,
) -> io::Result<()> {
    let relevant = val.u64_or("relevant", 0);
    write!(
        out,
        "ripwire-rs recall — \"{query}\" — {relevant} relevant of {} document files, best-first — total={relevant} shown={shown} capped={}",
        val.u64_or("documents", 0),
        u8::from(val.bool_or("capped", false))
    )?;
    for key in ["truncated", "generated_demoted"] {
        let n = val.u64_or(key, 0);
        if n > 0 {
            write!(out, " {key}={n}")?;
        }
    }
    let max_tokens = val.u64_or("max_tokens", 0);
    if est_tokens > max_tokens {
        out.write_all(b" over_ceiling=1")?;
    }
    write!(out, " max_tokens={max_tokens}")?;
    if let Some(share) = val.get("share_bytes").and_then(Value::as_u64) {
        write!(out, " share_bytes={share}")?;
    }
    write!(out, " est_tokens={est_tokens}\n\n")
}

/// Serialize `--recall` as plain text (upstream's rendering): header, ranked doc blocks, cap footer.
pub fn serialize_recall_text<W: Write>(query: &str, val: &Value, out: &mut W) -> io::Result<()> {
    let empty = Vec::new();
    let docs = val.array_any(&["docs"]).unwrap_or(&empty);
    let mut rest = Vec::new();
    for d in docs {
        write_recall_doc(&mut rest, d)?;
    }
    write_recall_footer(&mut rest, val)?;
    let mut est_tokens = 0u64;
    for _ in 0..4 {
        let mut header_len = CountingWriter::new(io::sink());
        write_recall_header(&mut header_len, query, val, docs.len(), est_tokens)?;
        let total_bytes = (header_len.count + rest.len()) as u64;
        let next = (total_bytes * 2).div_ceil(5);
        if next == est_tokens {
            break;
        }
        est_tokens = next;
    }
    write_recall_header(out, query, val, docs.len(), est_tokens)?;
    out.write_all(&rest)
}
