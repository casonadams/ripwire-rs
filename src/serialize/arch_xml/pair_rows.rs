use crate::serialize::escape::XmlEscapeExt;
use std::io::{self, Write};

struct StrAttrs<'a>(&'a [(&'a str, &'a str)]);

impl std::fmt::Display for StrAttrs<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0
            .iter()
            .try_for_each(|(key, value)| write!(f, " {key}=\"{}\"", value.xml_escaped()))
    }
}

/// One `<tag k="v" .../>` row per `(a, b)` pair, with `keys` naming the two escaped string attributes.
pub(super) fn write_pair_rows<'a, W: Write>(
    out: &mut W,
    tag: &str,
    keys: [&str; 2],
    rows: impl Iterator<Item = (&'a str, &'a str)>,
) -> io::Result<()> {
    rows.into_iter().try_for_each(|(a, b)| {
        writeln!(out, "  <{tag}{}/>", StrAttrs(&[(keys[0], a), (keys[1], b)]))
    })
}
