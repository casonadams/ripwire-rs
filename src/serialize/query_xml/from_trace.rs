use crate::serialize::JsonValueExt;
use serde_json::Value;
use std::io::{self, Write};

/// Serialize `--from-trace` to Ripwire XML.
pub fn serialize_from_trace_xml<W: Write>(val: &Value, out: &mut W) -> io::Result<()> {
    let empty = Vec::new();
    let suspects = val["suspects"].as_array().unwrap_or(&empty);
    writeln!(
        out,
        "<from-trace schema=\"ripwire-rs.from-trace/v1\" count=\"{}\">",
        suspects.len()
    )?;
    for s in suspects {
        let n = s.str_or("name", "");
        let p = s.str_any(&["path", "file"], "");
        let l = s.u64_or("line", 0);
        writeln!(out, "  <suspect n=\"{n}\" p=\"{p}\" l=\"{l}\"/>")?;
    }
    writeln!(out, "</from-trace>")
}
