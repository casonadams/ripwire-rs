use crate::serialize::escape::XmlEscapeExt;
use std::io::{self, Write};

use super::pair_rows::write_pair_rows;

pub struct ExercisesReport<'a> {
    pub test_path: &'a str,
    pub reached: &'a [(&'a str, &'a str)],
}

impl crate::serialize::XmlReport for ExercisesReport<'_> {
    crate::xml_report_meta!("exercises", "ripwire-rs.exercises/v1", root: true);
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        write!(
            out,
            " test=\"{}\" count=\"{}\"",
            self.test_path.xml_escaped(),
            self.reached.len()
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        write_pair_rows(out, "target", ["p", "n"], self.reached.iter().copied())
    }
}

/// Serialize `--exercises` to Ripwire XML.
pub fn serialize_exercises_xml<W: Write>(
    root: &str,
    test_path: &str,
    reached: &[(&str, &str)],
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlReport;
    ExercisesReport { test_path, reached }.write_xml_report(root, out)
}
