use crate::navigate::SymbolNavInfo;
use crate::serialize::{XmlEscapeExt, row_bytes};
use std::io::{self, Write};

pub(super) struct PackTaskBody {
    kind: String,
    line: u64,
    path_esc: String,
    name_esc: String,
    body: String,
}

/// Largest body one pack row may take; a larger body is truncated to fit its row.
const MAX_PACK_BODY_BYTES: usize = 3600;
/// A body is admitted only with at least this many bytes left: a shorter excerpt does not help the task.
const MIN_PACK_BODY_BYTES: usize = 256;
/// Marker appended to a truncated body; its bytes are part of that body's allowance.
const PACK_TRUNCATION_MARK: &str = "\n// ... [truncated to fit pack-task budget]";

pub(super) fn write_pack_caller<W: Write>(out: &mut W, c: &SymbolNavInfo) -> io::Result<()> {
    write!(
        out,
        "<s t=\"{}\" n=\"{}\" p=\"{}:{}\"/>",
        c.kind,
        c.name.xml_escaped(),
        c.path.xml_escaped(),
        c.line
    )
}

pub(super) fn write_pack_test<W: Write>(
    out: &mut W,
    t: &crate::gate::affected::AffectedTest,
) -> io::Result<()> {
    let partner = if t.is_partner { " partner=\"1\"" } else { "" };
    let run = t.run_attr();
    write!(
        out,
        "<t p=\"{}\"{partner} hops=\"{}\"{run}/>",
        t.path.xml_escaped(),
        t.hops
    )
}

pub fn truncate_body_to_budget(raw: &str, max_bytes: usize) -> String {
    if raw.len() <= max_bytes {
        return raw.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !raw.is_char_boundary(end) {
        end -= 1;
    }
    if let Some(nl) = raw[..end].rfind('\n') {
        end = nl;
    }
    format!("{}{PACK_TRUNCATION_MARK}", &raw[..end])
}

pub(super) fn pack_body_text(it: &serde_json::Value) -> Option<&str> {
    it.get("body")
        .or_else(|| it.get("sig"))
        .and_then(serde_json::Value::as_str)
}

pub(super) fn write_pack_sig_row<W: Write>(
    out: &mut W,
    idx: usize,
    it: &serde_json::Value,
) -> io::Result<()> {
    let raw_name = it["name"].as_str().unwrap_or("");
    let raw_path = it["path"].as_str().unwrap_or("");
    let n_esc = raw_name.xml_escaped();
    let p_esc = raw_path.xml_escaped();
    let l = it["line"].as_u64().unwrap_or(1);
    let cx = it["cx"].as_u64().unwrap_or(1);
    let ccx = it["ccx"].as_u64().unwrap_or(1);
    let in_deg = it["in"].as_u64().unwrap_or(0);
    let sig = it["sig"].as_str().unwrap_or(raw_name).xml_escaped();
    let r = idx + 1;
    write!(
        out,
        "<d l=\"{l}\" n=\"{n_esc}\" p=\"{p_esc}\" cx=\"{cx}\" ccx=\"{ccx}\" in=\"{in_deg}\" r=\"{r}\" next=\"--expand={p_esc}:{n_esc}\">{sig}</d>"
    )
}

/// Bodies of the admitted symbols in rank order, each truncated to what its row may take from `remaining`.
pub(super) fn admit_pack_bodies(
    admitted: &[serde_json::Value],
    remaining: &mut usize,
) -> Vec<PackTaskBody> {
    let mut bodies = Vec::new();
    for it in admitted {
        let Some(raw) = pack_body_text(it) else {
            continue;
        };
        let kind = it["kind"].as_str().unwrap_or("fn").to_string();
        let line = it["line"].as_u64().unwrap_or(1);
        let path_esc = it["path"].as_str().unwrap_or("").xml_escaped().to_string();
        let name_esc = it["name"].as_str().unwrap_or("").xml_escaped().to_string();
        let overhead = row_bytes(|w| {
            write!(
                w,
                "<b t=\"{kind}\" l=\"{line}\" p=\"{path_esc}\" n=\"{name_esc}\"><![CDATA[]]></b>"
            )
        });
        let cap = remaining.saturating_sub(overhead).min(MAX_PACK_BODY_BYTES);
        if cap < MIN_PACK_BODY_BYTES {
            break;
        }
        let body = if raw.len() <= cap {
            raw.to_string()
        } else {
            truncate_body_to_budget(raw, cap - PACK_TRUNCATION_MARK.len())
        };
        *remaining = remaining.saturating_sub(overhead + body.len());
        bodies.push(PackTaskBody {
            kind,
            line,
            path_esc,
            name_esc,
            body,
        });
    }
    bodies
}

pub(super) fn write_pack_bodies<W: Write>(out: &mut W, bodies: &[PackTaskBody]) -> io::Result<()> {
    for b in bodies {
        write!(
            out,
            "<b t=\"{}\" l=\"{}\" p=\"{}\" n=\"{}\">",
            b.kind, b.line, b.path_esc, b.name_esc
        )?;
        crate::serialize::write_cdata(out, &b.body)?;
        write!(out, "</b>")?;
    }
    Ok(())
}
