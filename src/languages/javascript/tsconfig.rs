pub(crate) const TS_CONFIG_MANIFESTS: &[&str] = &["tsconfig.json", "jsconfig.json"];

/// `compilerOptions.paths` of a tsconfig/jsconfig as `(pattern, first target)`, targets joined with
/// `baseUrl`; the file may hold comments and trailing commas.
pub(crate) fn parse_tsconfig_paths(content: &str) -> Vec<(String, String)> {
    let Ok(config) = serde_json::from_str::<serde_json::Value>(&strip_jsonc(content)) else {
        return Vec::new();
    };
    let options = &config["compilerOptions"];
    let base = options["baseUrl"].as_str().unwrap_or(".");
    let Some(paths) = options["paths"].as_object() else {
        return Vec::new();
    };
    paths
        .iter()
        .filter_map(|(pattern, targets)| {
            let target = targets.as_array()?.first()?.as_str()?;
            Some((pattern.clone(), crate::languages::normalize_lexical_path(base, target)))
        })
        .collect()
}

type CharStream<'a> = std::iter::Peekable<std::str::Chars<'a>>;

/// JSON text of a JSONC document: comments and trailing commas outside strings removed.
fn strip_jsonc(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut chars = content.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('"', _) => copy_json_string(&mut chars, &mut out),
            ('/', Some('/' | '*')) => skip_jsonc_comment(&mut chars),
            ('}' | ']', _) => {
                let kept = out.trim_end().len();
                if out[..kept].ends_with(',') {
                    out.truncate(kept - 1);
                }
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Copy a string literal whose opening quote was just read, through its closing quote.
fn copy_json_string(chars: &mut CharStream<'_>, out: &mut String) {
    out.push('"');
    while let Some(c) = chars.next() {
        out.push(c);
        match c {
            '\\' => out.extend(chars.next()),
            '"' => return,
            _ => {}
        }
    }
}

/// Skip a `//` or `/* */` comment whose leading `/` was just read.
fn skip_jsonc_comment(chars: &mut CharStream<'_>) {
    if chars.next() == Some('/') {
        while chars.next_if(|&c| c != '\n').is_some() {}
        return;
    }
    while let Some(c) = chars.next() {
        if c == '*' && chars.next_if_eq(&'/').is_some() {
            return;
        }
    }
}
