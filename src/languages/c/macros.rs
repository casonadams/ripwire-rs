fn strip_cpp_attribute_prefix(slice: &[u8]) -> &[u8] {
    let s = slice.trim_ascii_start();
    if s.starts_with(b"[[")
        && let Some(close) = s.windows(2).position(|w| w == b"]]")
    {
        return s[close + 2..].trim_ascii_start();
    }
    s
}

fn trailing_macro_before_semicolon(line: &[u8], trimmed_end: usize) -> Option<(usize, usize)> {
    if trimmed_end < 4 || line[trimmed_end - 1] != b';' {
        return None;
    }
    let before_semi = &line[..trimmed_end - 1];
    let tok_end = before_semi.iter().rposition(|&b| b != b' ' && b != b'\t')? + 1;
    let tok_start = before_semi[..tok_end]
        .iter()
        .rposition(|&b| !b.is_ascii_alphanumeric() && b != b'_')
        .map_or(0, |p| p + 1);
    let prefix = before_semi[..tok_start].trim_ascii_end();
    let after_sig = prefix.ends_with(b")") || prefix.ends_with(b"noexcept");
    let tok = std::str::from_utf8(&before_semi[tok_start..tok_end]).ok()?;
    (after_sig && crate::languages::is_screaming_snake(tok)).then_some((tok_start, tok_end))
}

fn is_semicolonless_macro_line(line: &[u8]) -> Option<(usize, usize)> {
    let start = line.iter().position(|&b| b != b' ' && b != b'\t')?;
    let trimmed_end = line
        .iter()
        .rposition(|&b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n'))?
        + 1;
    let slice = &line[start..trimmed_end];
    if slice.first() == Some(&b'#') {
        return None;
    }
    if let Some(span) = trailing_macro_before_semicolon(line, trimmed_end) {
        return Some(span);
    }
    let attr_tail = strip_cpp_attribute_prefix(slice);
    if let Ok(tok) = std::str::from_utf8(attr_tail)
        && crate::languages::is_screaming_snake(tok.trim())
    {
        let tail_start = trimmed_end - attr_tail.len();
        return Some((tail_start, trimmed_end));
    }
    if !slice.ends_with(b")") {
        return None;
    }
    let open_pos = slice.iter().position(|&b| b == b'(')?;
    let macro_name = std::str::from_utf8(&slice[..open_pos]).ok()?.trim();
    crate::languages::is_screaming_snake(macro_name).then_some((start, trimmed_end))
}

pub(crate) fn blank_c_family_macro_lines(source_bytes: &[u8]) -> Option<Vec<u8>> {
    let mut out = source_bytes.to_vec();
    let mut changed = false;
    let mut offset = 0usize;
    for line in source_bytes.split_inclusive(|&b| b == b'\n') {
        if let Some((s, e)) = is_semicolonless_macro_line(line) {
            out[offset + s..offset + e].fill(b' ');
            changed = true;
        }
        offset += line.len();
    }
    changed.then_some(out)
}
