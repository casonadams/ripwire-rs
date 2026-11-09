//! Dataform project `vars`: a model's config `name` and its `ref(...)` arguments may be the expression
//! `dataform.projectConfig.vars.KEY`, whose value lives in the project's `workflow_settings.yaml` or
//! `dataform.json`. The per-file `.sqlx` parse keeps the expression as the name; after all files are parsed
//! it is replaced by the value of the nearest settings file, a definition falls back to its file stem and a
//! reference to a key no settings file sets is dropped.
use crate::domain::Lang;
use crate::ingest::FileParseResult;
use std::collections::BTreeMap;

pub const PREFIX: &str = "dataform.projectConfig.vars.";

/// The `vars` of one settings file and the directory it configures.
pub struct ProjectVars {
    dir: String,
    vars: BTreeMap<String, String>,
}

/// The `dataform.projectConfig.vars.KEY` expression at the start of `text`, if any.
#[must_use]
pub fn expression(text: &str) -> Option<&str> {
    let key = text.strip_prefix(PREFIX)?;
    let len = key
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
        .count();
    (len > 0).then(|| &text[..PREFIX.len() + len])
}

/// The `vars` of `rel_path` when it is a Dataform settings file.
#[must_use]
pub fn project_vars(rel_path: &str, source: &[u8]) -> Option<ProjectVars> {
    let (dir, file) = rel_path.rsplit_once('/').unwrap_or(("", rel_path));
    let vars = match file {
        "workflow_settings.yaml" => yaml_vars(std::str::from_utf8(source).ok()?),
        "dataform.json" => json_vars(source)?,
        _ => return None,
    };
    Some(ProjectVars {
        dir: dir.to_string(),
        vars,
    })
}

/// Scalar entries of the top-level `vars:` block map; nested values are skipped.
fn yaml_vars(text: &str) -> BTreeMap<String, String> {
    let mut vars = BTreeMap::new();
    let mut in_block = false;
    for line in text.lines() {
        let body = line.trim();
        if !in_block {
            in_block = body == "vars:" && !line.starts_with([' ', '\t']);
        } else if body.is_empty() || body.starts_with('#') {
            continue;
        } else if !line.starts_with([' ', '\t']) {
            break;
        } else if let Some((key, value)) = body.split_once(':')
            && let Some(value) = yaml_scalar(value)
        {
            vars.insert(key.trim().trim_matches(['"', '\'']).to_string(), value);
        }
    }
    vars
}

fn yaml_scalar(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let value = match raw.chars().next()? {
        quote @ ('"' | '\'') => raw[1..].split(quote).next()?,
        _ => raw.split(" #").next()?.trim(),
    };
    (!value.is_empty()).then(|| value.to_string())
}

fn json_vars(source: &[u8]) -> Option<BTreeMap<String, String>> {
    let settings: serde_json::Value = serde_json::from_slice(source).ok()?;
    let vars = settings.get("vars")?.as_object()?;
    Some(
        vars.iter()
            .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string())))
            .collect(),
    )
}

fn vars_for<'a>(scopes: &'a [ProjectVars], path: &str) -> Option<&'a BTreeMap<String, String>> {
    scopes
        .iter()
        .filter(|scope| {
            scope.dir.is_empty()
                || path
                    .strip_prefix(&scope.dir)
                    .is_some_and(|rest| rest.starts_with('/'))
        })
        .max_by_key(|scope| scope.dir.len())
        .map(|scope| &scope.vars)
}

/// Replace `vars` expressions in the parsed `.sqlx` files; `paths[i]` is the path of `parsed[i]`.
pub fn resolve_names(
    parsed: &mut [Option<(Lang, FileParseResult)>],
    paths: &[&str],
    scopes: &[ProjectVars],
) {
    for (entry, path) in parsed.iter_mut().zip(paths) {
        let Some((_, file)) = entry else {
            continue;
        };
        if !path.ends_with(".sqlx") {
            continue;
        }
        let vars = vars_for(scopes, path);
        let value = |expr: &str| -> Option<String> {
            vars?.get(expr.strip_prefix(PREFIX)?).cloned()
        };
        for def in &mut file.defs {
            if def.name.starts_with(PREFIX) {
                def.name = value(&def.name).unwrap_or_else(|| super::sqlx::file_stem(path).to_string());
            }
        }
        file.refs.retain_mut(|reference| {
            if !reference.callee_name.starts_with(PREFIX) {
                return true;
            }
            value(&reference.callee_name).is_some_and(|resolved| {
                reference.callee_name = resolved;
                true
            })
        });
    }
}
