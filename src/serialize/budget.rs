use super::writer;

/// Bytes in four tokens of our XML answers. Measured with the o200k tokenizer (`docs/BASELINE.md`): 2.0 for
/// attribute-dense `--callers` rows to 3.2 for `--for`; the answers that take a token budget (map 2.6, `--for`
/// 2.7 to 3.0, pack-task 2.9, impact 2.75) centre on 2.75. A stated price and a budget use it, so the price
/// disclosed is close to the price paid.
const BYTES_PER_FOUR_TOKENS: usize = 11;

/// Tokens an answer of `bytes` costs.
#[must_use]
pub const fn est_tokens(bytes: usize) -> usize {
    bytes.saturating_mul(4).div_ceil(BYTES_PER_FOUR_TOKENS)
}

/// Bytes a budget of `tokens` pays for.
#[must_use]
pub const fn budget_bytes(tokens: usize) -> usize {
    tokens.saturating_mul(BYTES_PER_FOUR_TOKENS) / 4
}

/// Bytes `write` emits within the default minified output, where whitespace around a row's tags
/// is dropped; the measure a row costs against the output budget.
#[must_use]
pub fn row_bytes(write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>) -> usize {
    use std::io::Write as _;
    let mut buf = Vec::new();
    let mut minified = writer::MinifyingWriter::new(&mut buf);
    let _ = write(&mut minified);
    let _ = minified.flush();
    drop(minified);
    buf.trim_ascii().len()
}

/// Bytes a row costs when its cost is the text of its parts (names, paths, numbers).
#[must_use]
pub fn text_bytes(parts: &[&str]) -> usize {
    parts.iter().map(|part| part.len() + 1).sum()
}
