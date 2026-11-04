use crate::domain::IngestResult;
use serde_json::{Value, json};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Atomic edit operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditOp {
    ReplaceBody,
    InsertBefore,
    InsertAfter,
}

impl std::str::FromStr for EditOp {
    type Err = String;

    fn from_str(op_str: &str) -> Result<Self, Self::Err> {
        match op_str {
            "replace" | "replace_body" | "replace_symbol_body" => Ok(Self::ReplaceBody),
            "insert_before" | "insert_before_symbol" | "before" => Ok(Self::InsertBefore),
            "insert_after" | "insert_after_symbol" | "after" => Ok(Self::InsertAfter),
            other => Err(format!("unsupported edit op '{other}'")),
        }
    }
}

fn check_symlink(path: &Path) -> Result<(), Value> {
    if let Ok(meta) = fs::symlink_metadata(path)
        && meta.file_type().is_symlink()
    {
        return Err(json!({
            "applied": false,
            "error": format!("refusing to edit '{}': target is a symlink", path.display())
        }));
    }
    Ok(())
}

fn resolve_edit_target<'a>(
    ingest: &'a IngestResult,
    interner: &'a crate::domain::Interner,
    symbol_name: &str,
    path_hint: Option<&str>,
) -> Result<(&'a crate::domain::Symbol, &'a str), Value> {
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let matches: Vec<_> = ingest
        .symbols
        .iter()
        .filter(|sym| {
            view.sym_name(sym) == symbol_name
                && !view.sym_path(sym).is_empty()
                && path_hint.is_none_or(|h| view.sym_path(sym).contains(h))
        })
        .map(|sym| (sym, view.sym_path(sym)))
        .collect();

    if matches.is_empty() {
        return Err(json!({
            "applied": false,
            "error": format!("symbol '{symbol_name}' not found")
        }));
    }

    if matches.len() > 1 {
        let candidates: Vec<String> = matches
            .iter()
            .map(|(s, p)| format!("{p}:{}", s.line))
            .collect();
        return Err(json!({
            "applied": false,
            "error": format!("symbol '{symbol_name}' is ambiguous ({} definitions). Disambiguate with 'path'", matches.len()),
            "candidates": candidates
        }));
    }

    Ok(matches[0])
}

fn write_atomic_temp_file(target_path: &Path, content: &[u8]) -> std::io::Result<()> {
    let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
    let filename = target_path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("file");
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_name = format!(".{filename}.tmp.{}.{counter}", std::process::id());
    let temp_path = parent.join(temp_name);

    {
        let mut f = File::create(&temp_path)?;
        f.write_all(content)?;
        f.sync_all()?;
    }

    fs::rename(&temp_path, target_path)?;
    Ok(())
}

pub fn splice_file_bytes(
    original: &[u8],
    op: EditOp,
    sig_start: usize,
    end_byte: usize,
    payload: &[u8],
) -> Result<(Vec<u8>, usize, usize), Value> {
    let len = original.len();
    if sig_start > len || end_byte > len || sig_start > end_byte {
        return Err(json!({
            "applied": false,
            "error": format!("invalid symbol span [{sig_start}, {end_byte}) for file size {len}")
        }));
    }

    let mut out = Vec::with_capacity(len + payload.len());
    let new_start: usize;
    let new_end: usize;

    match op {
        EditOp::ReplaceBody => {
            out.extend_from_slice(&original[..sig_start]);
            new_start = out.len();
            out.extend_from_slice(payload);
            new_end = out.len();
            out.extend_from_slice(&original[end_byte..]);
        }
        EditOp::InsertBefore => {
            out.extend_from_slice(&original[..sig_start]);
            new_start = out.len();
            out.extend_from_slice(payload);
            new_end = out.len();
            out.extend_from_slice(&original[sig_start..]);
        }
        EditOp::InsertAfter => {
            out.extend_from_slice(&original[..end_byte]);
            new_start = out.len();
            out.extend_from_slice(payload);
            new_end = out.len();
            out.extend_from_slice(&original[end_byte..]);
        }
    }

    Ok((out, new_start, new_end))
}

/// Perform an atomic symbol edit (replace, insert_before, or insert_after).
fn validate_payload(payload: &str) -> Result<(), Value> {
    if payload.contains('\0') {
        Err(json!({
            "applied": false,
            "error": "payload contains NUL byte; refusing binary write"
        }))
    } else {
        Ok(())
    }
}

fn resolve_and_read_target_path(
    root: &str,
    file_rel_path: &str,
) -> Result<(std::path::PathBuf, Vec<u8>), Value> {
    let joined = Path::new(root).join(file_rel_path);
    let target_path = if joined.exists() {
        joined
    } else {
        std::path::PathBuf::from(file_rel_path)
    };
    let bytes = validate_and_read_target(&target_path)?;
    Ok((target_path, bytes))
}

fn validate_and_read_target(target_path: &Path) -> Result<Vec<u8>, Value> {
    check_symlink(target_path)?;
    fs::read(target_path).map_err(|e| {
        json!({
            "applied": false,
            "error": format!("cannot read file '{}': {e}", target_path.display())
        })
    })
}

pub struct EditTargetRequest<'a> {
    pub root: &'a str,
    pub op: EditOp,
    pub symbol_name: &'a str,
    pub path_hint: Option<&'a str>,
    pub payload: &'a str,
    pub dry_run: bool,
}

fn try_atomic_edit_in_root(
    ingest: &IngestResult,
    interner: &crate::domain::Interner,
    req: EditTargetRequest<'_>,
) -> Result<Value, Value> {
    validate_payload(req.payload)?;
    let (symbol, file_rel_path) =
        resolve_edit_target(ingest, interner, req.symbol_name, req.path_hint)?;
    let (target_path, original_bytes) = resolve_and_read_target_path(req.root, file_rel_path)?;
    let sig_start = symbol.def_start_byte() as usize;
    let end_byte = symbol.end_byte as usize;
    let (new_bytes, new_start, new_end) = splice_file_bytes(
        &original_bytes,
        req.op,
        sig_start,
        end_byte,
        req.payload.as_bytes(),
    )?;
    if !req.dry_run {
        write_atomic_temp_file(&target_path, &new_bytes).map_err(|e| {
            json!({
                "applied": false,
                "error": format!("failed to atomically write file '{}': {e}", target_path.display())
            })
        })?;
    }
    Ok(json!({
        "applied": !req.dry_run,
        "dry_run": req.dry_run,
        "file": file_rel_path,
        "symbol": req.symbol_name,
        "old_span": [sig_start, end_byte],
        "new_span": [new_start, new_end],
        "bytes_written": req.payload.len()
    }))
}

pub fn perform_atomic_edit_in_root(
    root: &str,
    ingest: &IngestResult,
    interner: &crate::domain::Interner,
    op: EditOp,
    symbol_name: &str,
    path_hint: Option<&str>,
    payload: &str,
) -> Value {
    perform_atomic_edit_with_options(
        EditTargetRequest {
            root,
            op,
            symbol_name,
            path_hint,
            payload,
            dry_run: false,
        },
        ingest,
        interner,
    )
}

pub fn perform_atomic_edit_with_options(
    req: EditTargetRequest<'_>,
    ingest: &IngestResult,
    interner: &crate::domain::Interner,
) -> Value {
    match try_atomic_edit_in_root(ingest, interner, req) {
        Ok(v) | Err(v) => v,
    }
}

fn parse_plan_edit_op(op_str: &str) -> Result<EditOp, String> {
    op_str.parse()
}

fn run_single_plan_item(
    item: &Value,
    root: &str,
    dry_run: bool,
    ingest: &IngestResult,
    interner: &crate::domain::Interner,
) -> Result<Value, Value> {
    let op_str = item["op"].as_str().unwrap_or("replace");
    let op = parse_plan_edit_op(op_str).map_err(|e| json!({ "applied": false, "error": e }))?;
    let sym = item["symbol"]
        .as_str()
        .or_else(|| item["target"].as_str())
        .ok_or_else(
            || json!({ "applied": false, "error": "missing 'symbol' field in edit item" }),
        )?;
    let path_hint = item["file"].as_str().or_else(|| item["path"].as_str());
    let payload = item["new_body"]
        .as_str()
        .or_else(|| item["text"].as_str())
        .or_else(|| item["payload"].as_str())
        .unwrap_or("");
    try_atomic_edit_in_root(
        ingest,
        interner,
        EditTargetRequest {
            root,
            op,
            symbol_name: sym,
            path_hint,
            payload,
            dry_run,
        },
    )
}

/// Execute a versioned JSON edit plan transactionally.
fn extract_plan_items(parsed: Value, dry_run: bool) -> Result<Vec<Value>, Value> {
    if let Some(arr) = parsed["edits"].as_array() {
        Ok(arr.clone())
    } else if parsed.is_object() && parsed.get("op").is_some() {
        Ok(vec![parsed])
    } else {
        Err(json!({
            "applied": false,
            "dry_run": dry_run,
            "error": "edit plan must contain an 'edits' array"
        }))
    }
}

fn run_plan_items_batch(
    items: &[Value],
    root: &str,
    dry_run: bool,
    ingest: &IngestResult,
    interner: &crate::domain::Interner,
) -> Result<Vec<Value>, Value> {
    let mut results = Vec::with_capacity(items.len());
    for item in items {
        results.push(run_single_plan_item(item, root, dry_run, ingest, interner)?);
    }
    Ok(results)
}

pub fn execute_edit_plan_json(
    root: &str,
    plan_raw: &str,
    dry_run: bool,
    ingest: &IngestResult,
    interner: &crate::domain::Interner,
) -> Value {
    let Ok(parsed) = serde_json::from_str::<Value>(plan_raw) else {
        return json!({
            "applied": false,
            "dry_run": dry_run,
            "error": "invalid JSON in edit plan"
        });
    };
    let items = match extract_plan_items(parsed, dry_run) {
        Ok(it) => it,
        Err(err) => return err,
    };
    let preflight = match run_plan_items_batch(&items, root, true, ingest, interner) {
        Ok(r) => r,
        Err(err) => return err,
    };
    let results = if dry_run {
        preflight
    } else {
        match run_plan_items_batch(&items, root, false, ingest, interner) {
            Ok(r) => r,
            Err(err) => return err,
        }
    };
    json!({
        "applied": !dry_run && !results.is_empty(),
        "dry_run": dry_run,
        "edits_count": results.len(),
        "edits": results
    })
}
